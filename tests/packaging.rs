//! Seam 3: the packaging scripts, run on temporary directories, and the macOS
//! app's Info.plist.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::Path;
use std::process::Command;

use pixpixpix::folder;

#[test]
fn the_formula_is_rendered_with_version_and_tarball_checksums() {
    let dist = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    fs::write(dist.path().join("pixpixpix-1.2.3-linux-x86_64.tar.gz"), b"").unwrap();
    fs::write(
        dist.path().join("pixpixpix-1.2.3-linux-aarch64.tar.gz"),
        b"abc",
    )
    .unwrap();
    let status = Command::new("sh")
        .arg("packaging/render-formula.sh")
        .arg("1.2.3")
        .args([dist.path(), out.path()])
        .status()
        .unwrap();
    assert!(status.success());
    let formula = fs::read_to_string(out.path().join("pixpixpix.rb")).unwrap();
    assert!(formula.contains("version \"1.2.3\""));
    // sha256 of the empty file and of "abc".
    assert!(
        formula.contains(
            "sha256 \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\""
        )
    );
    assert!(
        formula.contains(
            "sha256 \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
        )
    );
    assert!(!formula.contains("{{"));
    assert!(formula.contains("post_install_steps do"));
    assert!(!formula.contains("def post_install"));
}

#[test]
fn a_missing_tarball_fails_the_render() {
    let dist = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    fs::write(dist.path().join("pixpixpix-1.2.3-linux-x86_64.tar.gz"), b"").unwrap();
    let status = Command::new("sh")
        .arg("packaging/render-formula.sh")
        .arg("1.2.3")
        .args([dist.path(), out.path()])
        .status()
        .unwrap();
    assert!(!status.success());
}

/// Runs install-icon.sh with only the tools it needs on the PATH, so no desktop
/// cache of the machine running the tests is refreshed. Whether it succeeded.
fn install_icon(data_home: &Path, args: &[&str]) -> bool {
    let Ok(bin) = tempfile::tempdir() else {
        return false;
    };
    for tool in ["dirname", "cp", "mkdir", "rm", "grep", "sed"] {
        let Some(found) = ["/usr/bin", "/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists())
        else {
            return false;
        };
        if std::os::unix::fs::symlink(found, bin.path().join(tool)).is_err() {
            return false;
        }
    }
    Command::new("/bin/sh")
        .arg("packaging/linux/install-icon.sh")
        .args(args)
        .env("XDG_DATA_HOME", data_home)
        .env("PATH", bin.path())
        .status()
        .is_ok_and(|status| status.success())
}

const ENTRY: &str = "applications/io.github.paxel.pixpixpix.desktop";
const ICON: &str = "icons/hicolor/256x256/apps/pixpixpix.png";

#[test]
fn install_icon_copies_the_marked_entry_and_icons_and_takes_them_out_again() {
    let data = tempfile::tempdir().unwrap();
    assert!(install_icon(data.path(), &[]));
    assert_eq!(
        fs::read(data.path().join(ENTRY)).unwrap(),
        fs::read("packaging/linux/io.github.paxel.pixpixpix.desktop").unwrap()
    );
    assert_eq!(
        fs::read(data.path().join(ICON)).unwrap(),
        fs::read("assets/icon/icon_256.png").unwrap()
    );
    assert!(
        data.path()
            .join("icons/hicolor/scalable/apps/pixpixpix.svg")
            .exists()
    );
    assert!(!data.path().join("applications/mimeapps.list").exists());
    assert!(install_icon(data.path(), &["--uninstall"]));
    assert!(!data.path().join(ENTRY).exists());
    assert!(!data.path().join(ICON).exists());
}

#[test]
fn install_icon_leaves_a_foreign_entry_alone() {
    let data = tempfile::tempdir().unwrap();
    let entry = data.path().join(ENTRY);
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, "[Desktop Entry]\nExec=/mine\n").unwrap();
    assert!(install_icon(data.path(), &[]));
    assert_eq!(
        fs::read_to_string(&entry).unwrap(),
        "[Desktop Entry]\nExec=/mine\n"
    );
}

#[test]
fn the_cask_is_rendered_with_version_and_app_checksum() {
    let dist = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    fs::write(
        dist.path().join("pixpixpix-1.2.3-macos-arm64-app.zip"),
        b"abc",
    )
    .unwrap();
    let status = Command::new("sh")
        .arg("packaging/render-cask.sh")
        .arg("1.2.3")
        .args([dist.path(), out.path()])
        .status()
        .unwrap();
    assert!(status.success());
    let cask = fs::read_to_string(out.path().join("pixpixpix-cask.rb")).unwrap();
    assert!(cask.starts_with("cask \"pixpixpix\" do\n"));
    assert!(cask.contains("version \"1.2.3\""));
    assert!(
        cask.contains(
            "sha256 \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
        )
    );
    assert!(!cask.contains("{{"));
    assert!(cask.contains("depends_on arch: :arm64"));
    assert!(cask.contains("app \"pixpixpix.app\""));
    assert!(cask.contains("binary \"#{appdir}/pixpixpix.app/Contents/MacOS/pixpixpix\""));
    assert!(cask.contains("binary \"#{appdir}/pixpixpix.app/Contents/MacOS/pix3\""));
    // Deprecated since Homebrew 7.0.
    assert!(!cask.contains("flight do"));
}

#[test]
fn a_missing_app_zip_fails_the_cask_render() {
    let dist = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let status = Command::new("sh")
        .arg("packaging/render-cask.sh")
        .arg("1.2.3")
        .args([dist.path(), out.path()])
        .status()
        .unwrap();
    assert!(!status.success());
}

/// The type pixpixpix offers itself for, per supported extension, and whether
/// the app has to declare it because macOS does not.
fn uti(extension: &str) -> Option<(&'static str, bool)> {
    Some(match extension {
        "jpg" | "jpeg" | "jpe" => ("public.jpeg", false),
        "png" => ("public.png", false),
        "gif" => ("com.compuserve.gif", false),
        "webp" => ("org.webmproject.webp", false),
        "bmp" => ("com.microsoft.bmp", false),
        "tif" | "tiff" => ("public.tiff", false),
        "mpo" => ("public.mpo-image", true),
        "heic" => ("public.heic", false),
        "heif" | "hif" => ("public.heif", false),
        "avif" => ("public.avif", true),
        "mp4" => ("public.mpeg-4", false),
        "mov" => ("com.apple.quicktime-movie", false),
        "m4v" => ("com.apple.m4v-video", false),
        "mkv" => ("org.matroska.mkv", true),
        "webm" => ("org.webmproject.webm", true),
        "avi" => ("public.avi", false),
        "mts" | "m2ts" => ("public.avchd-mpeg-2-transport-stream", false),
        "3gp" => ("public.3gpp", false),
        "wmv" => ("com.microsoft.windows-media-wmv", true),
        "flv" => ("com.adobe.flash.video", true),
        "mpg" => ("public.mpeg", false),
        _ => return None,
    })
}

/// The part of `text` from `start` up to the next `end`.
fn section<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(start)?;
    rest.split_once(end).map(|(part, _)| part)
}

#[test]
fn the_app_offers_every_supported_type_in_open_with_but_claims_no_default() {
    let plist = fs::read_to_string("packaging/macos/Info.plist").unwrap();
    assert!(
        plist.contains("<key>CFBundleIdentifier</key><string>io.github.paxel.pixpixpix</string>")
    );
    assert!(plist.contains("<key>CFBundleExecutable</key><string>pixpixpix</string>"));
    let documents = section(
        &plist,
        "<key>CFBundleDocumentTypes</key>",
        "<key>UTImportedTypeDeclarations</key>",
    )
    .unwrap();
    let imported = section(&plist, "<key>UTImportedTypeDeclarations</key>", "</plist>").unwrap();
    let extensions = folder::IMAGE_EXTENSIONS
        .iter()
        .chain(folder::HEIF_EXTENSIONS)
        .chain(folder::VIDEO_EXTENSIONS);
    for extension in extensions {
        let (uti, declare) = uti(extension).unwrap_or_else(|| panic!("{extension} has no type"));
        assert!(
            documents.contains(&format!("<string>{uti}</string>")),
            "{uti} not offered"
        );
        if declare {
            assert!(imported.contains(&format!(
                "<key>UTTypeIdentifier</key><string>{uti}</string>"
            )));
            assert!(imported.contains(&format!(
                "<key>public.filename-extension</key><array><string>{extension}</string></array>"
            )));
        }
    }
    let ranks = documents.matches("<key>LSHandlerRank</key>").count();
    assert_eq!(
        ranks,
        documents.matches("<key>CFBundleTypeRole</key>").count()
    );
    assert_eq!(
        ranks,
        documents
            .matches("<key>LSHandlerRank</key><string>Alternate</string>")
            .count()
    );
}

/// What a run of the Flatpak install script did.
struct Install {
    succeeded: bool,
    /// Standard output and error.
    output: String,
    /// The calls of the fake curl and flatpak, one per line.
    log: String,
}

/// Runs packaging/flatpak/install.sh on a Linux `machine` with `home` as the
/// home, against fakes that log their calls: curl, whose latest release is
/// `latest`, flatpak (left out without `with_flatpak`) and uname. The real
/// tools on the PATH are only those the script needs, plus `extra_path`.
fn flatpak_install(
    home: &Path,
    machine: &str,
    latest: &str,
    with_flatpak: bool,
    extra_path: Option<&Path>,
) -> std::io::Result<Install> {
    let bin = tempfile::tempdir()?;
    for tool in ["mkdir", "mktemp", "rm", "chmod"] {
        let found = ["/usr/bin", "/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(tool))
            .find(|path| path.exists())
            .ok_or_else(|| std::io::Error::other(format!("no {tool}")))?;
        std::os::unix::fs::symlink(found, bin.path().join(tool))?;
    }
    let fake = |name: &str, body: &str| {
        let path = bin.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}"))?;
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
    };
    fake(
        "uname",
        "case $1 in -s) echo Linux ;; -m) echo \"$MACHINE\" ;; esac\n",
    )?;
    fake(
        "curl",
        r#"echo "curl $*" >>"$LOG"
out=/dev/null
effective=
while [ $# -gt 0 ]; do
  case $1 in
    -o) out=$2; shift ;;
    -w) effective=1; shift ;;
  esac
  shift
done
if [ -n "$effective" ]; then printf '%s' "$LATEST"; else echo bundle >"$out"; fi
"#,
    )?;
    if with_flatpak {
        fake(
            "flatpak",
            r#"echo "flatpak $*" >>"$LOG"
for last; do :; done
if [ "$1" = install ]; then read -r content <"$last"; echo "installed $content" >>"$LOG"; fi
"#,
        )?;
    }
    let log = home.join("calls.log");
    let mut path = bin.path().as_os_str().to_owned();
    if let Some(extra) = extra_path {
        path.push(":");
        path.push(extra);
    }
    let output = Command::new("/bin/sh")
        .arg("packaging/flatpak/install.sh")
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .env("MACHINE", machine)
        .env("LATEST", latest)
        .env("LOG", &log)
        .output()?;
    Ok(Install {
        succeeded: output.status.success(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        log: fs::read_to_string(&log).unwrap_or_default(),
    })
}

const LATEST: &str = "https://github.com/paxel/pixpixpix/releases/tag/v1.2.3";
const WRAPPER: &str = "#!/bin/sh\nexec flatpak run io.github.paxel.pixpixpix \"$@\"\n";

#[test]
fn the_install_script_installs_the_latest_bundle_with_its_runtime_and_both_wrappers() {
    let home = tempfile::tempdir().unwrap();
    let run = flatpak_install(home.path(), "x86_64", LATEST, true, None).unwrap();
    assert!(run.succeeded, "{}", run.output);
    let calls: Vec<&str> = run.log.lines().collect();
    assert_eq!(calls.len(), 5, "{}", run.log);
    assert_eq!(
        calls[0],
        "curl -fsSLI -o /dev/null -w %{url_effective} https://github.com/paxel/pixpixpix/releases/latest"
    );
    assert!(calls[1].starts_with("curl -fSL --progress-bar -o "));
    assert!(calls[1].ends_with(
        "/pixpixpix-1.2.3-x86_64.flatpak https://github.com/paxel/pixpixpix/releases/download/v1.2.3/pixpixpix-1.2.3-x86_64.flatpak"
    ));
    assert_eq!(
        calls[2],
        "flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"
    );
    assert!(
        calls[3].starts_with("flatpak install --user -y --noninteractive --reinstall --bundle ")
    );
    assert!(calls[3].ends_with("/pixpixpix-1.2.3-x86_64.flatpak"));
    // The downloaded bundle is what gets installed.
    assert_eq!(calls[4], "installed bundle");
    for name in ["pixpixpix", "pix3"] {
        let wrapper = home.path().join(".local/bin").join(name);
        assert_eq!(fs::read_to_string(&wrapper).unwrap(), WRAPPER);
        let mode =
            std::os::unix::fs::PermissionsExt::mode(&fs::metadata(&wrapper).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o755);
    }
    assert!(run.output.contains("is not on your PATH"), "{}", run.output);
}

#[test]
fn the_install_script_picks_the_aarch64_bundle_and_notes_nothing_with_the_bin_dir_on_the_path() {
    let home = tempfile::tempdir().unwrap();
    let bin_dir = home.path().join(".local/bin");
    let run = flatpak_install(home.path(), "arm64", LATEST, true, Some(&bin_dir)).unwrap();
    assert!(run.succeeded, "{}", run.output);
    assert!(run.log.contains("/pixpixpix-1.2.3-aarch64.flatpak https://github.com/paxel/pixpixpix/releases/download/v1.2.3/pixpixpix-1.2.3-aarch64.flatpak\n"));
    assert!(
        !run.output.contains("is not on your PATH"),
        "{}",
        run.output
    );
}

#[test]
fn the_install_script_stops_with_the_reason_before_touching_anything() {
    let cases = [
        ("x86_64", LATEST, false, "flatpak is not installed"),
        ("riscv64", LATEST, true, "no bundle for riscv64"),
        (
            "x86_64",
            "https://github.com/paxel/pixpixpix/releases",
            true,
            "no release found",
        ),
    ];
    for (machine, latest, with_flatpak, reason) in cases {
        let home = tempfile::tempdir().unwrap();
        let run = flatpak_install(home.path(), machine, latest, with_flatpak, None).unwrap();
        assert!(!run.succeeded);
        assert!(run.output.contains(reason), "{}", run.output);
        assert!(!run.log.contains("flatpak"), "{}", run.log);
        assert!(!home.path().join(".local/bin").exists());
    }
}

#[test]
fn the_flatpak_command_starts_the_binary_with_the_host_data_dir_for_the_trash() {
    let app = tempfile::tempdir().unwrap();
    fs::create_dir_all(app.path().join("bin")).unwrap();
    fs::create_dir_all(app.path().join("libexec")).unwrap();
    fs::copy(
        "packaging/flatpak/pixpixpix.sh",
        app.path().join("bin/pixpixpix"),
    )
    .unwrap();
    let binary = app.path().join("libexec/pixpixpix");
    fs::write(&binary, "#!/bin/sh\necho \"$XDG_DATA_HOME $*\"\n").unwrap();
    fs::set_permissions(&binary, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let run = |host: Option<&str>| {
        let mut command = Command::new(app.path().join("bin/pixpixpix"));
        command
            .args(["a.jpg", "b c.png"])
            .env("HOME", "/home/u")
            .env(
                "XDG_DATA_HOME",
                "/home/u/.var/app/io.github.paxel.pixpixpix/data",
            )
            .env_remove("HOST_XDG_DATA_HOME");
        if let Some(host) = host {
            command.env("HOST_XDG_DATA_HOME", host);
        }
        String::from_utf8(command.output().unwrap().stdout).unwrap()
    };
    assert_eq!(run(None), "/home/u/.local/share a.jpg b c.png\n");
    assert_eq!(run(Some("/data")), "/data a.jpg b c.png\n");
}
