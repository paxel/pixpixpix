#!/usr/bin/env node
// Builds the documentation site, as cat(a)log builds its own: site/pages/*.html
// fragments with a front-matter comment -> site-dist/ with full pages, the
// assets, a search index and a 404 page. English only. The overview's
// `<!-- whats-new -->` becomes What's new of the release the site is built
// for; a What's new page lists every release.
// Zero dependencies; Node 22+. Usage:
//   node tool/site/build.js [--src site] [--out site-dist] [--base /] [--serve [port]]

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { fileURLToPath } from "node:url";
import { REPO_ROOT, decodeEntities, escapeHtml, readVersion, slugify, stripTags } from "./common.js";
import { generatedPages, latestNews } from "./generated.js";
export const REPO_URL = "https://github.com/paxel/pixpixpix";
export const SITE_URL = "https://paxel.github.io/pixpixpix/";

/** The sections of the navigation. */
export const TRACKS = [
  { id: "start", label: "Start" },
  { id: "use", label: "Using pixpixpix" },
  { id: "reference", label: "Reference" },
];

/** The chrome's words; the pages carry their own. */
export const UI = {
  overview: "Overview",
  noChapters: "No chapters yet",
  onThisPage: "On this page",
  search: "Search the docs",
  searchLabel: "Search the documentation",
  searchResults: "Search results",
  skip: "Skip to content",
  openNav: "Open navigation",
  closeNav: "Close navigation",
  switchTheme: "Switch theme",
  toDark: "Switch to dark theme",
  toLight: "Switch to light theme",
  copyLink: "Copy link to",
  copyLinkTitle: "Copy link to this section",
  notFound: "Page not found",
  notFoundLede: "There is nothing at this address. The page may have moved, or the link is older than the site.",
  notFoundHelp: "Try the search box above (press /) or start from the",
  overviewLink: "overview",
  projectLinks: "Project links",
  whatsNew: "What's new",
  versionTitle: "pixpixpix version",
  versionsTitle: "Documentation versions",
  latest: "latest",
  noticeFor: "Documentation for v{v}.",
  noticeRead: "Read the latest, v{l}",
  noMatches: "No matches",
  searchUnavailable: "Search unavailable",
  results: "results",
  result: "result",
  linkCopied: "Link copied",
  copyFailed: "Could not copy automatically; the heading is selected",
};

/** Where the overview shows What's new of the current release. */
export const WHATS_NEW_MARK = "<!-- whats-new -->";

const FRONT_MATTER_KEYS = ["title", "track", "order", "summary"];

const FONTS_HREF =
  "https://fonts.googleapis.com/css2?family=Noto+Sans:ital,wght@0,400;0,600;1,400&family=Noto+Serif:wght@400;600&display=swap";

export { REPO_ROOT, escapeHtml, decodeEntities, stripTags, slugify, readVersion };

function collectIds(html) {
  const ids = new Set();
  const re = /\sid\s*=\s*(?:"([^"]+)"|'([^']+)'|([^\s>"']+))/g;
  let m;
  while ((m = re.exec(html)) !== null) ids.add(m[1] ?? m[2] ?? m[3]);
  return ids;
}

// ---------------------------------------------------------------------------
// Front matter: <!-- page: title="…" track="use" order=1 summary="…" -->

export function parseFrontMatter(source, file = "page") {
  const match = /^\s*<!--\s*page:([\s\S]*?)-->/.exec(source);
  if (!match) throw new Error(`${file}: missing front matter (<!-- page: … -->)`);
  const meta = {};
  const pair = /^([a-z]+)\s*=\s*(?:"([^"]*)"|([^\s"]+))(?:\s+|$)/;
  let rest = match[1].trim();
  while (rest.length) {
    const m = pair.exec(rest);
    if (!m) {
      throw new Error(
        `${file}: cannot read front matter at "${rest.slice(0, 40)}": expected key="value" ` +
          `pairs; a bare value is a single token, values with spaces must be quoted`,
      );
    }
    if (!FRONT_MATTER_KEYS.includes(m[1])) {
      throw new Error(`${file}: unknown front-matter key "${m[1]}"`);
    }
    meta[m[1]] = m[2] !== undefined ? m[2] : m[3];
    rest = rest.slice(m[0].length);
  }
  if (!meta.title) throw new Error(`${file}: front matter has no title`);
  if (meta.track && !TRACKS.some((t) => t.id === meta.track)) {
    throw new Error(`${file}: unknown track "${meta.track}"`);
  }
  const order = meta.order === undefined ? 0 : Number(meta.order);
  if (Number.isNaN(order)) throw new Error(`${file}: order "${meta.order}" is not a number`);
  return {
    meta: {
      title: meta.title,
      track: meta.track || "",
      order,
      summary: meta.summary || "",
    },
    body: source.slice(match[0].length).trim(),
  };
}

// ---------------------------------------------------------------------------
// Headings: ids, copy-link anchors, table of contents, section text.

export function processHeadings(body) {
  const used = collectIds(body);
  const emitted = new Set();
  const headings = [];
  const matches = [];
  const re = /<(h[23])(\s[^>]*)?>([\s\S]*?)<\/\1>/g;
  let m;
  while ((m = re.exec(body)) !== null) {
    const [whole, tag, attrs = "", inner] = m;
    const text = stripTags(inner);
    const authored = /\sid\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>"']+))/.exec(attrs);
    const base = authored ? (authored[1] ?? authored[2] ?? authored[3]) : slugify(text);
    let id = base;
    let n = 1;
    while (authored ? emitted.has(id) : used.has(id)) id = `${base}-${++n}`;
    emitted.add(id);
    used.add(id);
    headings.push({ level: Number(tag[1]), id, text });
    const rest = attrs.replace(/\sid\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>"']+)/, "");
    const anchor =
      `<a class="anchor" href="#${id}" aria-label="${escapeHtml(UI.copyLink)} ${escapeHtml(text)}" ` +
      `title="${escapeHtml(UI.copyLinkTitle)}">#</a>`;
    matches.push({
      start: m.index,
      end: m.index + whole.length,
      html: `<${tag} id="${id}"${rest}>${inner}${anchor}</${tag}>`,
    });
  }
  let html = "";
  let cursor = 0;
  const sections = [];
  for (let i = 0; i < matches.length; i++) {
    html += body.slice(cursor, matches[i].start) + matches[i].html;
    cursor = matches[i].end;
    const next = i + 1 < matches.length ? matches[i + 1].start : body.length;
    sections.push(stripTags(body.slice(cursor, next)));
  }
  html += body.slice(cursor);
  const intro = stripTags(body.slice(0, matches.length ? matches[0].start : body.length));
  return { html, headings, intro, sections };
}

// ---------------------------------------------------------------------------
// Pages, nav and search index. A page's url is its file name under pages/.

export function loadPages(srcDir) {
  const root = path.join(srcDir, "pages");
  if (!fs.existsSync(root)) throw new Error(`${srcDir}: no pages/ folder`);
  const pages = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith(".html")) continue;
    const { meta, body } = parseFrontMatter(fs.readFileSync(path.join(root, entry.name), "utf8"), entry.name);
    pages.push({ ...meta, path: entry.name, url: entry.name, body });
  }
  return sortPages(pages);
}

/** The overview first, then the tracks in order, then page order. */
export function sortPages(pages) {
  const trackNo = (id) => (id ? TRACKS.findIndex((t) => t.id === id) : -1);
  return pages.sort(
    (a, b) => trackNo(a.track) - trackNo(b.track) || a.order - b.order || a.url.localeCompare(b.url),
  );
}

export function buildNav(pages) {
  return TRACKS.map((track) => ({
    id: track.id,
    label: track.label,
    pages: pages
      .filter((p) => p.track === track.id)
      .sort((a, b) => a.order - b.order || a.title.localeCompare(b.title))
      .map(({ title, url, order, summary }) => ({ title, url, order, summary })),
  }));
}

export function buildIndex(pages) {
  const trackLabel = (id) => TRACKS.find((t) => t.id === id)?.label || "";
  return pages.map((page) => {
    const { headings, intro, sections } = processHeadings(page.body);
    return {
      title: page.title,
      track: trackLabel(page.track),
      url: page.url,
      summary: page.summary,
      intro,
      headings: headings.map((h, i) => ({ id: h.id, text: h.text, body: sections[i] || "" })),
    };
  });
}

// ---------------------------------------------------------------------------
// Rendering

function renderNav(nav, currentUrl) {
  const link = (p) =>
    `<li><a href="${p.url}"${p.url === currentUrl ? ' aria-current="page"' : ""}>${escapeHtml(p.title)}</a></li>`;
  const home = `<a class="nav-home" href="index.html"${currentUrl === "index.html" ? ' aria-current="page"' : ""}>${escapeHtml(UI.overview)}</a>`;
  const tracks = nav
    .map(
      (t) =>
        `<section class="nav-track" aria-label="${escapeHtml(t.label)}"><p class="eyebrow">${escapeHtml(t.label)}</p>` +
        (t.pages.length
          ? `<ul>${t.pages.map(link).join("")}</ul>`
          : `<p class="nav-empty">${escapeHtml(UI.noChapters)}</p>`) +
        `</section>`,
    )
    .join("");
  return home + tracks;
}

function renderToc(headings) {
  if (!headings.length) return "";
  const items = headings
    .map((h) => `<li class="toc-l${h.level}"><a href="#${h.id}">${escapeHtml(h.text)}</a></li>`)
    .join("");
  return `<aside class="toc" aria-label="${escapeHtml(UI.onThisPage)}">\n<p class="eyebrow">${escapeHtml(UI.onThisPage)}</p><ol>${items}</ol>\n</aside>\n`;
}

// ---------------------------------------------------------------------------
// Versions: each release has its own copy of the site, under /vX.Y.Z/, the
// latest one also at the root. Each entry is { version, base, pages, latest };
// `pages` lists the URLs that version has, so the menu sends a reader to the
// same page where it exists and to the overview where it does not.

/** The URL of `pageUrl` in another version of the site, or of its overview when that version lacks the page. */
export function versionHref(entry, pageUrl) {
  const target = pageUrl && entry.pages.includes(pageUrl) ? pageUrl : "index.html";
  return `${entry.base}${target}`;
}

function renderVersionMenu(version, versions, currentUrl) {
  const chip = `<span class="chip" title="${escapeHtml(UI.versionTitle)}">v${escapeHtml(version)}</span>`;
  if (!versions.length) return chip;
  const items = versions
    .map((entry) => {
      const label = `v${escapeHtml(entry.version)}${entry.latest ? ` (${escapeHtml(UI.latest)})` : ""}`;
      return entry.version === version
        ? `<li><span aria-current="true">${label}</span></li>`
        : `<li><a href="${versionHref(entry, currentUrl)}">${label}</a></li>`;
    })
    .join("");
  return (
    `<details class="version-menu"><summary class="chip" title="${escapeHtml(UI.versionsTitle)}">v${escapeHtml(version)}</summary>` +
    `<ul aria-label="${escapeHtml(UI.versionsTitle)}">${items}</ul></details>`
  );
}

/** A one-line pointer to the latest release on every page of an older one. */
function renderVersionNotice(version, versions, currentUrl) {
  const latest = versions.find((entry) => entry.latest);
  if (!latest || latest.version === version) return "";
  return (
    `<p class="version-notice">${escapeHtml(UI.noticeFor.replace("{v}", version))} ` +
    `<a href="${versionHref(latest, currentUrl)}">${escapeHtml(UI.noticeRead.replace("{l}", latest.version))}</a>.</p>\n`
  );
}

function shell({ title, summary, rel, root, nav, currentUrl, eyebrow, main, toc, version, whatsNew, versions = [] }) {
  const i18n = {
    noMatches: UI.noMatches,
    searchUnavailable: UI.searchUnavailable,
    results: UI.results,
    result: UI.result,
    linkCopied: UI.linkCopied,
    copyFailed: UI.copyFailed,
    openNav: UI.openNav,
    closeNav: UI.closeNav,
    toLight: UI.toLight,
    toDark: UI.toDark,
  };
  return `<!doctype html>
<html lang="en" data-root="${root}" data-i18n='${escapeHtml(JSON.stringify(i18n)).replace(/'/g, "&#39;")}'>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)} — pixpixpix</title>
${summary ? `<meta name="description" content="${escapeHtml(summary)}">\n` : ""}<script>try{var t=localStorage.getItem("pixpixpix-theme");if(t==="light"||t==="dark"){document.documentElement.setAttribute("data-theme",t)}}catch(e){}</script>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="${FONTS_HREF}">
<link rel="stylesheet" href="${rel}assets/site.css">
<link rel="icon" href="${rel}assets/icon.png" type="image/png">
</head>
<body>
<a class="skip" href="#content">${escapeHtml(UI.skip)}</a>
<div class="sr-only" id="live" aria-live="polite" aria-atomic="true"></div>
<header class="site-header">
<button class="nav-toggle" type="button" aria-controls="sidebar" aria-expanded="false" aria-label="${escapeHtml(UI.openNav)}"><span class="nav-toggle-bars" aria-hidden="true"></span></button>
<a class="brand" href="${rel}index.html"><img class="mark" src="${rel}assets/icon.png" width="28" height="28" alt=""><span class="brand-name">pixpixpix</span></a>
${renderVersionMenu(version, versions, currentUrl)}
<div class="search" role="search">
<input class="search-input" id="search" type="search" placeholder="${escapeHtml(UI.search)}" aria-label="${escapeHtml(UI.searchLabel)}" role="combobox" aria-autocomplete="list" aria-controls="search-results" aria-expanded="false" autocomplete="off" spellcheck="false">
<kbd class="search-kbd" aria-hidden="true">/</kbd>
<ul class="search-results" id="search-results" role="listbox" aria-label="${escapeHtml(UI.searchResults)}" hidden></ul>
</div>
<button class="theme-toggle" type="button" aria-label="${escapeHtml(UI.toDark)}" title="${escapeHtml(UI.switchTheme)}">
<svg class="icon-sun" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="12" cy="12" r="4" fill="currentColor"/><g stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></g></svg>
<svg class="icon-moon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" fill="currentColor"/></svg>
</button>
</header>
<div class="layout">
<nav class="sidebar" id="sidebar" aria-label="Documentation">
${renderNav(nav, currentUrl)}
</nav>
<main class="content" id="content" tabindex="-1">
${renderVersionNotice(version, versions, currentUrl)}${eyebrow ? `<p class="eyebrow track-eyebrow">${escapeHtml(eyebrow)}</p>\n` : ""}${main}
</main>
${toc}</div>
<footer class="site-footer">
<span>pixpixpix <span class="chip" title="${escapeHtml(UI.versionTitle)}">v${escapeHtml(version)}</span></span>
<nav aria-label="${escapeHtml(UI.projectLinks)}"><a href="${REPO_URL}">GitHub</a>${whatsNew ? `<a href="${rel}whats-new.html">${escapeHtml(UI.whatsNew)}</a>` : ""}</nav>
</footer>
<script src="${rel}assets/site.js" defer></script>
</body>
</html>
`;
}

export function renderPage(page, { navs, version, whatsNew, versions = [] }) {
  const { html, headings } = processHeadings(page.body);
  const body = html.replace(/<table(\s[^>]*)?>[\s\S]*?<\/table>/g, (t) => `<div class="table-wrap">${t}</div>`);
  const track = TRACKS.find((t) => t.id === page.track);
  return shell({
    title: page.title,
    summary: page.summary,
    rel: "",
    root: "./",
    nav: navs,
    currentUrl: page.url,
    eyebrow: track ? track.label : "",
    main: `<h1>${escapeHtml(page.title)}</h1>\n${body}`,
    toc: renderToc(headings),
    version,
    whatsNew,
    versions,
  });
}

export function render404({ navs, version, whatsNew, base = "/", versions = [] }) {
  const main = `<h1>${escapeHtml(UI.notFound)}</h1>
<p class="lede">${escapeHtml(UI.notFoundLede)}</p>
<p>${escapeHtml(UI.notFoundHelp)} <a href="${base}index.html">${escapeHtml(UI.overviewLink)}</a>.</p>`;
  // Served for any missing address, so every link is absolute.
  const nav = navs.map((t) => ({ ...t, pages: t.pages.map((p) => ({ ...p, url: `${base}${p.url}` })) }));
  return shell({
    title: UI.notFound,
    summary: "",
    rel: base,
    root: base,
    nav,
    currentUrl: "",
    eyebrow: "404",
    main,
    toc: "",
    version,
    whatsNew,
    versions,
  }).replace(/<a class="nav-home" href="index\.html"/, `<a class="nav-home" href="${base}index.html"`);
}

// ---------------------------------------------------------------------------
// Link check

const ATTR_LINK = /\s(?:xlink:)?(?:href|src)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>"']+))/g;
const SRCSET = /\ssrcset\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
const CSS_URL = /url\(\s*(?:"([^"]*)"|'([^']*)'|([^)"']*))\s*\)/g;

/** Every link (href, src, srcset candidates, CSS url()) in one output file. */
export function extractLinks(file, content) {
  const links = [];
  if (file.endsWith(".css")) {
    for (const m of content.matchAll(CSS_URL)) links.push((m[1] ?? m[2] ?? m[3]).trim());
    return links;
  }
  if (!file.endsWith(".html")) return links;
  for (const m of content.matchAll(ATTR_LINK)) links.push(m[1] ?? m[2] ?? m[3]);
  for (const m of content.matchAll(SRCSET)) {
    for (const candidate of (m[1] ?? m[2]).split(",")) {
      const url = candidate.trim().split(/\s+/)[0];
      if (url) links.push(url);
    }
  }
  return links;
}

const EXTERNAL = /^(https?:|mailto:|data:|tel:)/i;

/** files: Map<relativePath, content> of every output file. Throws on the first broken link or anchor. */
export function checkLinks(files, { base = "/", knownTargets = new Set() } = {}) {
  const ids = new Map();
  for (const [file, html] of files) if (file.endsWith(".html")) ids.set(file, collectIds(html));
  const asIndex = (target) => `${target.replace(/\/$/, "")}/index.html`;
  const exists = (target) => files.has(target) || files.has(asIndex(target));
  for (const [file, content] of files) {
    for (const link of extractLinks(file, content)) {
      if (EXTERNAL.test(link)) continue;
      const hashAt = link.indexOf("#");
      const fragment = hashAt >= 0 ? link.slice(hashAt + 1) : "";
      let target = (hashAt >= 0 ? link.slice(0, hashAt) : link).replace(/\?.*$/, "");
      if (target.startsWith("/")) {
        if (knownTargets.has(target)) continue; // a file of another version
        if (!target.startsWith(base)) {
          throw new Error(`Broken link in ${file}: "${link}" is outside the site base ${base}`);
        }
        target = target.slice(base.length) || "index.html";
      } else if (target) {
        target = path.posix.normalize(path.posix.join(path.posix.dirname(file), target));
        if (target === "." || target === "./") target = "index.html";
      }
      let resolved = target || file;
      if (target) {
        if (!exists(resolved)) {
          throw new Error(`Broken link in ${file}: "${link}" does not resolve to a file in the output`);
        }
        if (!files.has(resolved)) resolved = asIndex(resolved);
      }
      if (fragment && !ids.get(resolved)?.has(fragment)) {
        throw new Error(`Broken link in ${file}: "${link}" points to a missing anchor #${fragment}`);
      }
    }
  }
}

// Authored pages must not use root-relative links: on GitHub Pages the site
// lives under /pixpixpix/, so "/install.html" breaks there however it looks locally.
function rejectRootRelativeLinks(page) {
  for (const link of extractLinks(page.url, page.body)) {
    if (link.startsWith("/")) {
      throw new Error(
        `Broken link in ${page.url}: "${link}" is root-relative; the site is served under a ` +
          `path on GitHub Pages, use a relative link`,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Build

function copyDir(from, to, files) {
  fs.mkdirSync(to, { recursive: true });
  for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
    const src = path.join(from, entry.name);
    const dst = path.join(to, entry.name);
    if (entry.isDirectory()) copyDir(src, dst, files);
    else {
      fs.copyFileSync(src, dst);
      files.push(dst);
    }
  }
}

const isInside = (dir, parent) => {
  const rel = path.relative(parent, dir);
  return rel !== "" && !rel.startsWith("..") && !path.isAbsolute(rel);
};

/** The output folder is wiped; refuse anything that is not clearly a build output. */
function guardOutDir(outDir, srcDir) {
  const tmp = fs.realpathSync(os.tmpdir());
  const out = path.resolve(outDir);
  if (!isInside(out, REPO_ROOT) && !isInside(out, tmp)) {
    throw new Error(`Refusing to wipe ${out}: the output folder must be inside the repository or the temp dir`);
  }
  const src = path.resolve(srcDir);
  if (out === src || isInside(src, out)) {
    throw new Error(`Refusing to wipe ${out}: it contains the source folder ${src}`);
  }
}

/** Every page of the site in nav order and checked, What's new of `version` on the overview. */
export function collectPages({ srcDir = path.join(REPO_ROOT, "site"), generated = [], news = "" } = {}) {
  const pages = loadPages(srcDir);
  for (const page of generated) {
    if (pages.some((p) => p.url === page.url)) {
      throw new Error(`${page.url}: an authored page shadows the generated page`);
    }
    pages.push(page);
  }
  sortPages(pages);
  const overview = pages.find((p) => p.url === "index.html");
  if (!overview) throw new Error(`${srcDir}: no pages/index.html`);
  if (news) {
    if (!overview.body.includes(WHATS_NEW_MARK)) throw new Error(`index.html: no ${WHATS_NEW_MARK}`);
    overview.body = overview.body.replace(WHATS_NEW_MARK, news);
  }
  for (const page of pages) rejectRootRelativeLinks(page);
  return pages;
}

/** The icon the site shows, the app's own. */
const ICON = path.join(REPO_ROOT, "assets", "icon", "icon_256.png");

export function buildSite({
  srcDir = path.join(REPO_ROOT, "site"),
  outDir = path.join(REPO_ROOT, "site-dist"),
  version = readVersion(fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8")),
  base = "/",
  assetsDir = path.join(srcDir, "assets"),
  // Copied over the assets: a release's own screenshots, which show its app.
  overlayDir = null,
  pages = null,
  generated = [],
  news = "",
  versions = [],
  knownTargets = new Set(),
} = {}) {
  if (!base.startsWith("/") || !base.endsWith("/")) {
    throw new Error(`--base must start and end with "/": got "${base}"`);
  }
  guardOutDir(outDir, srcDir);
  if (!pages) pages = collectPages({ srcDir, generated, news });
  const navs = buildNav(pages);
  const whatsNew = pages.some((p) => p.url === "whats-new.html");
  const ctx = { navs, version, base, versions, whatsNew };

  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });

  const files = new Map();
  for (const page of pages) files.set(page.url, renderPage(page, ctx));
  files.set("404.html", render404(ctx));

  const copied = [];
  if (fs.existsSync(assetsDir)) copyDir(assetsDir, path.join(outDir, "assets"), copied);
  if (overlayDir && fs.existsSync(overlayDir)) {
    copyDir(overlayDir, path.join(outDir, "assets", path.basename(overlayDir)), copied);
  }
  if (fs.existsSync(ICON)) {
    fs.copyFileSync(ICON, path.join(outDir, "assets", "icon.png"));
    copied.push(path.join(outDir, "assets", "icon.png"));
  }
  for (const file of copied) {
    const rel = path.relative(outDir, file).split(path.sep).join("/");
    if (!files.has(rel)) files.set(rel, /\.(css|js|html|svg)$/.test(rel) ? fs.readFileSync(file, "utf8") : "");
  }

  checkLinks(files, { base, knownTargets });

  for (const [rel, html] of files) {
    if (!rel.endsWith(".html")) continue;
    const dest = path.join(outDir, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, html);
  }
  fs.writeFileSync(path.join(outDir, "search-index.json"), JSON.stringify(buildIndex(pages), null, 0));
  fs.writeFileSync(path.join(outDir, ".nojekyll"), "");
  return { pages: pages.length, outDir, version };
}

// ---------------------------------------------------------------------------
// Preview server

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".txt": "text/plain; charset=utf-8",
};

export function resolveRequest(root, urlPath) {
  let decoded;
  try {
    decoded = decodeURIComponent(urlPath);
  } catch {
    return null;
  }
  let file = path.resolve(root, `.${decoded}`);
  const rel = path.relative(root, file);
  if (rel.startsWith("..") || path.isAbsolute(rel)) return null;
  if (fs.existsSync(file) && fs.statSync(file).isDirectory()) file = path.join(file, "index.html");
  return fs.existsSync(file) ? file : null;
}

export function serve(outDir, port, host = "127.0.0.1") {
  const root = path.resolve(outDir);
  const server = http.createServer((req, res) => {
    const urlPath = new URL(req.url, "http://localhost").pathname;
    let file = resolveRequest(root, urlPath);
    let status = 200;
    if (!file) {
      file = path.join(root, "404.html");
      status = 404;
    }
    res.writeHead(status, { "content-type": MIME[path.extname(file)] || "application/octet-stream" });
    fs.createReadStream(file).pipe(res);
  });
  server.listen(port, host, () => {
    const { port: bound } = server.address();
    console.log(`Serving ${root} at http://${host}:${bound}/ (Ctrl+C to stop)`);
  });
  return server;
}

// ---------------------------------------------------------------------------
// CLI

function parseArgs(argv) {
  const opts = { serve: false, port: 8080 };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--serve") {
      opts.serve = true;
      if (/^\d+$/.test(argv[i + 1] || "")) opts.port = Number(argv[++i]);
    } else if (arg === "--src") opts.srcDir = path.resolve(argv[++i]);
    else if (arg === "--out") opts.outDir = path.resolve(argv[++i]);
    else if (arg === "--base") opts.base = argv[++i];
    else throw new Error(`Unknown argument: ${arg}`);
  }
  return opts;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const opts = parseArgs(process.argv.slice(2));
    const version = readVersion(fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8"));
    const result = buildSite({ ...opts, version, generated: generatedPages(), news: latestNews(version) });
    console.log(`Built ${result.pages} pages (v${result.version}) into ${result.outDir}`);
    if (opts.serve) serve(result.outDir, opts.port);
  } catch (err) {
    console.error(`build-site: ${err.message}`);
    process.exit(1);
  }
}
