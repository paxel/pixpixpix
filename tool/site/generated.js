// Pages and parts the build writes from the repository's own files: What's
// new from the changelogs, as a page of every release and as the block of
// the current release on the overview.
import fs from "node:fs";
import path from "node:path";
import { REPO_ROOT, escapeHtml } from "./common.js";

/** Markdown inline: `code`, **bold**, _em_ and [text](url). */
export function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>')
    .replace(/(^|\s)_([^_]+)_(?=[\s.,;:]|$)/g, "$1<em>$2</em>");
}

/** The `## [version] - date` sections of a changelog, each with its categories and bullet lines. */
export function parseChangelog(text, file = "changelog") {
  const sections = [];
  let current = null;
  let category = null;
  for (const line of text.split("\n")) {
    const head = /^## \[([^\]]+)\](?:\s*-\s*(\S+))?/.exec(line);
    if (head) {
      current = { version: head[1], date: head[2] || "", categories: [] };
      sections.push(current);
      category = null;
      continue;
    }
    if (!current) continue;
    const cat = /^### (\w+)/.exec(line);
    if (cat) {
      category = { name: cat[1], items: [] };
      current.categories.push(category);
      continue;
    }
    if (line.startsWith("- ") && category) category.items.push(line.slice(2).trim());
    else if (line.startsWith("  ") && category && category.items.length) {
      category.items[category.items.length - 1] += " " + line.trim();
    }
  }
  if (!sections.length) throw new Error(`${file}: no version sections found`);
  return sections;
}

/** The current release in CHANGELOG.md, then the older ones in OLDER_CHANGES.md. */
const CHANGELOGS = ["CHANGELOG.md", "OLDER_CHANGES.md"];

export function readChangelogs(repoRoot = REPO_ROOT) {
  const sections = [];
  for (const file of CHANGELOGS) {
    const full = path.join(repoRoot, file);
    if (fs.existsSync(full)) sections.push(...parseChangelog(fs.readFileSync(full, "utf8"), file));
  }
  return sections;
}

/** The id of a release's heading on the What's new page. */
export const releaseId = (version) => `v${version.replace(/\./g, "-")}`;

function renderCategories(section, level) {
  return section.categories
    .filter((c) => c.items.length)
    .map(
      (c) =>
        `<h${level}>${escapeHtml(c.name)}</h${level}>\n<ul>\n` +
        `${c.items.map((i) => `<li>${inline(i)}</li>`).join("\n")}\n</ul>`,
    )
    .join("\n");
}

/** The What's new page: every release, newest first. */
export function whatsNewPage(repoRoot = REPO_ROOT) {
  const parts = readChangelogs(repoRoot).map((section) => {
    const date = section.date ? ` <small>${escapeHtml(section.date)}</small>` : "";
    return `<h2 id="${releaseId(section.version)}">${escapeHtml(section.version)}${date}</h2>\n${renderCategories(section, 3)}`;
  });
  return {
    path: "whats-new.html",
    url: "whats-new.html",
    track: "reference",
    order: 9,
    title: "What's new",
    summary: "Every release of pixpixpix, newest first, with what changed.",
    alias: "",
    body: `<p class="lede">Every release, newest first, in the words of the changelog.</p>\n${parts.join("\n")}`,
  };
}

/** The block the overview opens with: what the release `version` brings. */
export function latestNews(version, repoRoot = REPO_ROOT) {
  const section = readChangelogs(repoRoot).find((s) => s.version === version);
  if (!section) throw new Error(`CHANGELOG.md: no section for the version ${version}`);
  return (
    `<section class="whats-new" aria-labelledby="whats-new">\n` +
    `<h2 id="whats-new">What's new in ${escapeHtml(version)}</h2>\n` +
    `${renderCategories(section, 3)}\n` +
    `<p><a href="whats-new.html">Every release</a></p>\n</section>`
  );
}

export function generatedPages(repoRoot = REPO_ROOT) {
  return [whatsNewPage(repoRoot)];
}
