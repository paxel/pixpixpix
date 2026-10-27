// The site build: front matter, headings, the overview's What's new, the
// search index, the 404 page and the link check. Run with
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
import { generatedPages, latestNews, parseChangelog } from "./generated.js";

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
