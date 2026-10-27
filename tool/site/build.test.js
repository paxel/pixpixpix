// The site build: front matter, headings, the overview's What's new, the
// search index, the 404 page, the link check and the versions. Run with
// `node --test "tool/site/*.test.js"`.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import {
  REPO_ROOT,
  buildSite,
  checkLinks,
  collectPages,
  parseFrontMatter,
  processHeadings,
  readVersion,
  versionHref,
} from "./build.js";
import { generatedPages, inline, latestNews, parseChangelog } from "./generated.js";
import { buildAll, compareVersions, planVersions } from "./build-all.js";
import { execFileSync } from "node:child_process";

const PAGE = (title, body = "<p>Text.</p>", extra = "") =>
  `<!-- page: title="${title}" ${extra} -->\n${body}\n`;

/** A tiny site in a temp dir: an overview and the pages given as {name: source}. */
function tinySite(pages = {}, overview = PAGE("Home", "<!-- whats-new -->")) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pixpixpix-site-"));
  const src = path.join(dir, "site");
  fs.mkdirSync(path.join(src, "assets"), { recursive: true });
  fs.mkdirSync(path.join(src, "pages"), { recursive: true });
  fs.writeFileSync(path.join(src, "assets", "site.css"), "body{}");
  fs.writeFileSync(path.join(src, "assets", "site.js"), "");
  fs.writeFileSync(path.join(src, "pages", "index.html"), overview);
  for (const [name, source] of Object.entries(pages)) {
    fs.writeFileSync(path.join(src, "pages", name), source);
  }
  return { dir, src, out: path.join(dir, "site-dist") };
}

test("front matter: keys, order, unknown keys and tracks are refused", () => {
  const { meta, body } = parseFrontMatter('<!-- page: title="A page" track="use" order=3 summary="Short." -->\n<p>x</p>');
  assert.deepEqual(meta, { title: "A page", track: "use", order: 3, summary: "Short." });
  assert.equal(body, "<p>x</p>");
  assert.throws(() => parseFrontMatter("<p>no front matter</p>", "x.html"), /x\.html: missing front matter/);
  assert.throws(() => parseFrontMatter('<!-- page: title="A" track="nope" -->', "x.html"), /unknown track "nope"/);
  assert.throws(() => parseFrontMatter('<!-- page: title="A" colour="red" -->', "x.html"), /unknown front-matter key/);
  assert.throws(() => parseFrontMatter('<!-- page: track="use" -->', "x.html"), /no title/);
});

test("headings get ids, copy anchors and a table of contents", () => {
  const { html, headings, intro, sections } = processHeadings(
    "<p>Intro.</p><h2>Trash and undo</h2><p>Body one.</p><h3>Same</h3><h3>Same</h3>",
  );
  assert.deepEqual(
    headings.map((h) => [h.level, h.id, h.text]),
    [
      [2, "trash-and-undo", "Trash and undo"],
      [3, "same", "Same"],
      [3, "same-2", "Same"],
    ],
  );
  assert.match(html, /<h2 id="trash-and-undo">Trash and undo<a class="anchor"/);
  assert.equal(intro, "Intro.");
  assert.equal(sections[0], "Body one.");
});

test("the version comes from the package section of Cargo.toml", () => {
  assert.equal(readVersion('[package]\nname = "x"\nversion = "0.3.0"\n\n[dependencies]\nversion = "9.9.9"\n'), "0.3.0");
  assert.throws(() => readVersion('[package]\nname = "x"\n'), /no package version/);
  assert.equal(readVersion(fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8")).split(".").length, 3);
});

test("a build writes the pages, the search index and the 404, What's new on the overview", () => {
  const { src, out } = tinySite({
    "install.html": PAGE("Install", '<p>See <a href="index.html">home</a>.</p><h2>Homebrew</h2><p>brew install.</p>', 'track="start"'),
  });
  const news = '<section class="whats-new"><h2 id="whats-new">What\'s new in 9.9.9</h2></section>';
  const result = buildSite({ srcDir: src, outDir: out, version: "9.9.9", base: "/pixpixpix/", news });
  assert.equal(result.pages, 2);
  const install = fs.readFileSync(path.join(out, "install.html"), "utf8");
  assert.match(install, /<html lang="en"/);
  assert.match(install, /v9\.9\.9/);
  assert.match(install, /<p class="eyebrow track-eyebrow">Start<\/p>/);
  assert.doesNotMatch(install, /lang-switch/);
  assert.match(fs.readFileSync(path.join(out, "index.html"), "utf8"), /What's new in 9\.9\.9/);
  const index = JSON.parse(fs.readFileSync(path.join(out, "search-index.json"), "utf8"));
  assert.deepEqual(index.map((p) => p.url), ["index.html", "install.html"]);
  assert.equal(index[1].headings[0].body, "brew install.");
  assert.match(fs.readFileSync(path.join(out, "404.html"), "utf8"), /Page not found/);
  assert.ok(fs.existsSync(path.join(out, ".nojekyll")));
});

test("an overview without the What's new mark fails the build", () => {
  const { src } = tinySite({}, PAGE("Home"));
  assert.throws(() => collectPages({ srcDir: src, news: "<p>new</p>" }), /no <!-- whats-new -->/);
});

test("a broken link or anchor fails the build, and a root-relative one too", () => {
  const files = new Map([
    ["index.html", '<a href="x.html">x</a>'],
    ["x.html", '<h2 id="a">A</h2><a href="index.html#nope">n</a>'],
  ]);
  assert.throws(() => checkLinks(files), /missing anchor #nope/);
  files.set("x.html", '<a href="gone.html">g</a>');
  assert.throws(() => checkLinks(files), /"gone\.html" does not resolve/);
  const { src } = tinySite({ "x.html": PAGE("X", '<a href="/index.html">root</a>', 'track="use"') });
  assert.throws(() => collectPages({ srcDir: src }), /is root-relative/);
});

test("the changelog parses, and the overview gets the current release", () => {
  const log = parseChangelog("## [0.3.0]\n\n### Added\n- One.\n  more\n- Two.\n\n## [0.2.0] - 2026-10-27\n\n### Fixed\n- Three.\n");
  assert.deepEqual(log.map((s) => [s.version, s.date, s.categories.map((c) => [c.name, c.items])]), [
    ["0.3.0", "", [["Added", ["One. more", "Two."]]]],
    ["0.2.0", "2026-10-27", [["Fixed", ["Three."]]]],
  ]);
  assert.throws(() => latestNews("99.0.0"), /no section for the version 99\.0\.0/);
});

test("the repository's own site builds and links, What's new of every release included", () => {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), "pixpixpix-site-dist-"));
  const version = readVersion(fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8"));
  buildSite({ outDir: out, base: "/pixpixpix/", version, generated: generatedPages(), news: latestNews(version) });
  const news = fs.readFileSync(path.join(out, "whats-new.html"), "utf8");
  for (const release of ["0.1.0", "0.2.0", version]) assert.match(news, new RegExp(`id="v${release.replace(/\./g, "-")}"`));
  assert.match(fs.readFileSync(path.join(out, "index.html"), "utf8"), new RegExp(`What's new in ${version.replace(/\./g, "\\.")}`));
});

test("the version menu keeps the page where the other version has it, else the overview", () => {
  const old = { version: "0.3.0", base: "/pixpixpix/v0.3.0/", pages: ["index.html", "install.html"], latest: false };
  assert.equal(versionHref(old, "install.html"), "/pixpixpix/v0.3.0/install.html");
  assert.equal(versionHref(old, "trash.html"), "/pixpixpix/v0.3.0/index.html");
});

test("every release is built, the working tree replacing its own tag or ahead of them", () => {
  const plan = planVersions({ tags: ["v0.3.0", "v0.3.1", "v0.4.0"], workingVersion: "0.4.0" });
  assert.deepEqual(
    plan.map((e) => [e.version, e.source, e.latest]),
    [
      ["0.4.0", "worktree", true],
      ["0.3.1", "tag", false],
      ["0.3.0", "tag", false],
    ],
  );
  const ahead = planVersions({ tags: ["v0.3.0"], workingVersion: "0.4.0" });
  assert.deepEqual(ahead.map((e) => e.version), ["0.4.0", "0.3.0"]);
  assert.deepEqual(["0.9.0", "0.10.0", "1.0.0"].sort(compareVersions), ["1.0.0", "0.10.0", "0.9.0"]);
});

/** A git repository with a site: a tag without pages, one with, and a newer working tree. */
function releasedRepo() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pixpixpix-releases-"));
  const run = (...args) => execFileSync("git", args, { cwd: dir, stdio: "pipe" });
  const write = (rel, text) => {
    fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
    fs.writeFileSync(path.join(dir, rel), text);
  };
  const release = (version) => {
    write("Cargo.toml", `[package]\nname = "pixpixpix"\nversion = "${version}"\n`);
    write("CHANGELOG.md", `# Changelog\n\n## [${version}]\n\n### Added\n\n- Feature of ${version}.\n`);
  };
  run("init", "-q");
  run("config", "user.email", "test@example.com");
  run("config", "user.name", "Test");
  release("0.2.0");
  run("add", "-A");
  run("commit", "-qm", "0.2.0");
  run("tag", "v0.2.0");
  release("0.3.0");
  write("site/assets/site.css", "body{}");
  write("site/assets/site.js", "");
  write("site/pages/index.html", PAGE("Home", "<!-- whats-new -->"));
  write("site/pages/old.html", PAGE("Old", '<p>Only in 0.3.0.</p><img src="assets/screenshots/a.png" alt="">', 'track="use"'));
  write("site/assets/screenshots/a.png", "the app of 0.3.0");
  run("add", "-A");
  run("commit", "-qm", "0.3.0");
  run("tag", "v0.3.0");
  release("0.4.0");
  fs.rmSync(path.join(dir, "site/pages/old.html"));
  // 0.3.0's entry, now older, links to a page only 0.3.0 has.
  write(
    "OLDER_CHANGES.md",
    "# Older changes\n\n## [0.3.0] - 2026-11-01\n\n### Added\n\n- [Old](https://paxel.github.io/pixpixpix/old.html) things.\n",
  );
  write("site/assets/screenshots/a.png", "the app of 0.4.0");
  return dir;
}

test("build-all writes the latest at the root, every release with pages under its version", () => {
  const repo = releasedRepo();
  const out = path.join(repo, "site-dist");
  const result = buildAll({ repoRoot: repo, outDir: out, base: "/pixpixpix/" });
  assert.deepEqual(result.versions, ["0.4.0", "0.3.0"]);
  assert.match(fs.readFileSync(path.join(out, "index.html"), "utf8"), /What's new in 0\.4\.0/);
  const old = fs.readFileSync(path.join(out, "v0.3.0", "old.html"), "utf8");
  assert.match(old, /Only in 0\.3\.0/);
  assert.match(old, /class="version-notice"/);
  // The menu sends the old page to the latest overview, which lacks it.
  assert.match(old, /href="\/pixpixpix\/index\.html">v0\.4\.0 \(latest\)/);
  assert.ok(!fs.existsSync(path.join(out, "v0.2.0")));
  // An older release's entry leads to its own docs.
  const news = fs.readFileSync(path.join(out, "whats-new.html"), "utf8");
  assert.match(news, /<a href="\/pixpixpix\/v0\.3\.0\/old\.html">Old<\/a>/);
  // Each version shows its own app.
  assert.equal(fs.readFileSync(path.join(out, "v0.3.0", "assets", "screenshots", "a.png"), "utf8"), "the app of 0.3.0");
  assert.equal(fs.readFileSync(path.join(out, "assets", "screenshots", "a.png"), "utf8"), "the app of 0.4.0");
  const versions = JSON.parse(fs.readFileSync(path.join(out, "versions.json"), "utf8"));
  assert.deepEqual(versions.map((v) => v.version), ["0.4.0", "0.3.0"]);
});

test("a link to the site in a changelog entry leads to that page of the version built", () => {
  assert.equal(
    inline("See [the trash](https://paxel.github.io/pixpixpix/trash-and-undo.html#undo)."),
    'See <a href="trash-and-undo.html#undo">the trash</a>.',
  );
  assert.equal(inline("[docs](https://paxel.github.io/pixpixpix/)"), '<a href="index.html">docs</a>');
  assert.equal(inline("[GitHub](https://github.com/paxel)"), '<a href="https://github.com/paxel">GitHub</a>');
  // Another release's folder, or the latest site for a release without docs.
  assert.equal(inline("[t](https://paxel.github.io/pixpixpix/a.html)", "/pixpixpix/v0.3.0/"), '<a href="/pixpixpix/v0.3.0/a.html">t</a>');
  assert.equal(inline("[t](https://paxel.github.io/pixpixpix/a.html)", null), '<a href="https://paxel.github.io/pixpixpix/a.html">t</a>');
  // A page or an anchor the version lacks fails the build.
  const files = new Map([["whats-new.html", inline("[gone](https://paxel.github.io/pixpixpix/index.html#nowhere)")], ["index.html", ""]]);
  assert.throws(() => checkLinks(files), /missing anchor #nowhere/);
});
