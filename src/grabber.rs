//! First frames of videos, grabbed by one long-lived helper process for the
//! whole app, several at a time. When the helper crashes, it is restarted for
//! new grabs, and the grabs it was running are retried one at a time, each in
//! a helper of its own: the file that crashes that one alone is the culprit
//! and fails, the others get their frames.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::decode::DecodedImage;
use crate::helper::{self, Reader, Writer};
use crate::video;

/// What a grab gives.
type Grabbed = Result<DecodedImage, String>;

/// How often a wait for a frame looks at the cancellation token.
const POLL: Duration = Duration::from_millis(50);

/// The error of the file that crashes the helper on its own.
pub const CRASHED: &str = "the file crashed the video player";

/// A grab waiting for its frame.
struct Job {
    path: PathBuf,
    /// Set once the waiter gave up; a retry skips it then.
    cancel: Arc<AtomicBool>,
    reply: Sender<Grabbed>,
}

/// The running shared helper.
struct Helper {
    stdin: ChildStdin,
    /// Its grabs by id, until answered; what is left when it crashes is
    /// retried.
    jobs: Arc<Mutex<HashMap<u64, Job>>>,
    /// Gone once it ended.
    alive: Arc<AtomicBool>,
}

/// Grabs first frames through helpers made by its command factory.
pub struct Grabber {
    make: Box<dyn Fn() -> std::io::Result<Command> + Send + Sync>,
    helper: Mutex<Option<Helper>>,
    next: Mutex<u64>,
    /// The grabs to retry alone, one at a time, in order.
    retries: Mutex<Option<Sender<Job>>>,
}

/// The grabber of the app.
pub fn shared() -> &'static Arc<Grabber> {
    static SHARED: OnceLock<Arc<Grabber>> = OnceLock::new();
    SHARED.get_or_init(|| Grabber::new(|| helper::command(helper::GRAB)))
}

impl Grabber {
    /// A grabber whose helpers `make` starts.
    pub fn new(make: impl Fn() -> std::io::Result<Command> + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            make: Box::new(make),
            helper: Mutex::new(None),
            next: Mutex::new(0),
            retries: Mutex::new(None),
        })
    }

    /// The first frame of the video at `path`, or `None` once `cancel` is
    /// set. Waits as long as the grab takes.
    pub fn grab(self: &Arc<Self>, path: &Path, cancel: &AtomicBool) -> Option<Grabbed> {
        let (reply, answer) = channel();
        let job = Job {
            path: path.to_path_buf(),
            cancel: Arc::new(AtomicBool::new(false)),
            reply,
        };
        let given_up = Arc::clone(&job.cancel);
        let id = self.send(job);
        let result = wait(&answer, cancel);
        if result.is_none() {
            given_up.store(true, Ordering::Relaxed);
            self.cancel(id);
        }
        result
    }

    /// Hands `job` to the shared helper, started if none runs, and returns
    /// its id. A helper that cannot start fails the job at once.
    fn send(self: &Arc<Self>, job: Job) -> u64 {
        let id = {
            let Ok(mut next) = self.next.lock() else {
                return 0;
            };
            *next = next.wrapping_add(1);
            *next
        };
        let Ok(mut running) = self.helper.lock() else {
            return id;
        };
        if running
            .as_ref()
            .is_none_or(|h| !h.alive.load(Ordering::Relaxed))
        {
            match self.start() {
                Ok(started) => *running = Some(started),
                Err(error) => {
                    // The waiter may have given up already.
                    let _ = job.reply.send(Err(error));
                    return id;
                }
            }
        }
        let Some(helper) = running.as_mut() else {
            return id;
        };
        let message = Writer::new(GRAB)
            .u64(id)
            .rest(helper::path_bytes(&job.path));
        let Ok(mut jobs) = helper.jobs.lock() else {
            return id;
        };
        // Its reader takes the jobs left under this lock once it ended.
        if !helper.alive.load(Ordering::Relaxed) {
            drop(jobs);
            self.retry(job);
            return id;
        }
        jobs.insert(id, job);
        drop(jobs);
        // A helper that cannot take it has crashed; its reader retries the
        // job.
        let _ = helper::write_message(&mut helper.stdin, &message);
        id
    }

    /// Tells the shared helper to stop the grab `id`.
    fn cancel(&self, id: u64) {
        let Ok(mut running) = self.helper.lock() else {
            return;
        };
        if let Some(helper) = running.as_mut() {
            let gone = helper
                .jobs
                .lock()
                .ok()
                .and_then(|mut jobs| jobs.remove(&id));
            if gone.is_some() {
                let _ =
                    helper::write_message(&mut helper.stdin, &Writer::new(CANCEL).u64(id).done());
            }
        }
    }

    /// Starts a shared helper and the thread reading its answers.
    fn start(self: &Arc<Self>) -> Result<Helper, String> {
        let mut child = (self.make)()
            .and_then(helper::spawn)
            .map_err(|error| format!("the video player cannot start: {error}"))?;
        let (Some(stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("the video player cannot start".to_owned());
        };
        let jobs: Arc<Mutex<HashMap<u64, Job>>> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let (answered, flag, grabber) = (Arc::clone(&jobs), Arc::clone(&alive), Arc::clone(self));
        std::thread::spawn(move || {
            while let Ok(Some(message)) = helper::read_message(&mut stdout) {
                let Some((id, result)) = decode_answer(&message) else {
                    break;
                };
                let job = answered.lock().ok().and_then(|mut jobs| jobs.remove(&id));
                if let Some(job) = job {
                    let _ = job.reply.send(result);
                }
            }
            drop(stdout);
            let _ = child.wait();
            let left: Vec<Job> = answered
                .lock()
                .map(|mut jobs| {
                    flag.store(false, Ordering::Relaxed);
                    jobs.drain().map(|(_, job)| job).collect()
                })
                .unwrap_or_default();
            for job in left {
                grabber.retry(job);
            }
        });
        Ok(Helper { stdin, jobs, alive })
    }

    /// Queues `job` to be grabbed alone, after the others queued.
    fn retry(self: &Arc<Self>, job: Job) {
        let Ok(mut retries) = self.retries.lock() else {
            return;
        };
        let queue = retries.get_or_insert_with(|| {
            let (queue, jobs) = channel::<Job>();
            let grabber = Arc::clone(self);
            std::thread::spawn(move || {
                for job in jobs {
                    if !job.cancel.load(Ordering::Relaxed) {
                        let result = grabber.alone(&job.path);
                        let _ = job.reply.send(result);
                    }
                }
            });
            queue
        });
        let _ = queue.send(job);
    }

    /// Grabs `path` in a helper of its own; its crash makes `path` the
    /// culprit.
    fn alone(&self, path: &Path) -> Grabbed {
        let mut child = (self.make)()
            .and_then(helper::spawn)
            .map_err(|error| format!("the video player cannot start: {error}"))?;
        let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("the video player cannot start".to_owned());
        };
        let message = Writer::new(GRAB).u64(0).rest(helper::path_bytes(path));
        let answer = helper::write_message(&mut stdin, &message)
            .and_then(|()| helper::read_message(&mut stdout));
        drop(stdin);
        let _ = child.wait();
        match answer {
            Ok(Some(message)) => decode_answer(&message)
                .map_or_else(|| Err(CRASHED.to_owned()), |(_, result)| result),
            _ => Err(CRASHED.to_owned()),
        }
    }
}

/// Waits for the answer until `cancel` is set.
fn wait(answer: &Receiver<Grabbed>, cancel: &AtomicBool) -> Option<Grabbed> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        match answer.recv_timeout(POLL) {
            Ok(result) => return Some(result),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Some(Err("the video player stopped".to_owned()));
            }
        }
    }
}

/// The helper's side: grabs every file asked for on stdin in a thread of its
/// own and answers on stdout, until stdin closes.
pub fn serve() -> ExitCode {
    let mut stdin = std::io::stdin();
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let running: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>> = Arc::default();
    while let Ok(Some(message)) = helper::read_message(&mut stdin) {
        let mut reader = Reader::new(&message);
        match (reader.u8(), reader.u64()) {
            (Some(GRAB), Some(id)) => {
                let path = helper::path_from(reader.rest());
                let cancel = Arc::new(AtomicBool::new(false));
                if let Ok(mut running) = running.lock() {
                    running.insert(id, Arc::clone(&cancel));
                }
                let (out, running) = (Arc::clone(&out), Arc::clone(&running));
                std::thread::spawn(move || {
                    let result = video::grab_here(&path, &cancel);
                    if let Ok(mut running) = running.lock() {
                        running.remove(&id);
                    }
                    if let Some(result) = result
                        && let Ok(mut out) = out.lock()
                    {
                        let _ = helper::write_message(&mut *out, &encode_answer(id, &result));
                    }
                });
            }
            (Some(CANCEL), Some(id)) => {
                if let Some(cancel) = running.lock().ok().and_then(|mut r| r.remove(&id)) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            _ => return ExitCode::FAILURE,
        }
    }
    ExitCode::SUCCESS
}

const GRAB: u8 = 0;
const CANCEL: u8 = 1;
const FRAME: u8 = 0;
const FAILED: u8 = 1;

fn encode_answer(id: u64, result: &Grabbed) -> Vec<u8> {
    match result {
        Ok(image) => Writer::new(FRAME).u64(id).image(image),
        Err(message) => Writer::new(FAILED).u64(id).rest(message.as_bytes()),
    }
}

fn decode_answer(message: &[u8]) -> Option<(u64, Grabbed)> {
    let mut reader = Reader::new(message);
    let (tag, id) = (reader.u8()?, reader.u64()?);
    let result = match tag {
        FRAME => Ok(reader.image()?),
        FAILED => Err(String::from_utf8_lossy(reader.rest()).into_owned()),
        _ => return None,
    };
    Some((id, result))
}
