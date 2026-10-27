#!/usr/bin/env node
// Builds every published version of the documentation site into one output
// folder, as cat(a)log does: the latest release at the root and every
// release under /vX.Y.Z/, with a versions.json at the root. A release is a
// git tag vX.Y.Z whose tree carries site/pages/index.html; the working tree
// is one more version (the newest, normally) and takes the place of the tag
// with the same version. Every version is built with today's build.js and
// today's assets from that release's own pages and changelogs, so the
// version menu looks the same everywhere and an old version describes its
// own app. A release that no longer builds fails the whole build. Zero
// dependencies beyond git and tar; Node 22+. Usage:
//   node tool/site/build-all.js [--out site-dist] [--base /]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { REPO_ROOT, buildSite, collectPages, readVersion } from "./build.js";
import { generatedPages, latestNews } from "./generated.js";

/** The paths of a release a site build reads. */
export const RELEASE_PATHS = ["site", "Cargo.toml", "CHANGELOG.md", "OLDER_CHANGES.md"];

const RELEASE_TAG = /^v(\d+)\.(\d+)\.(\d+)$/;

function git(repoRoot, args, options = {}) {
  return execFileSync("git", args, { cwd: repoRoot, encoding: "utf8", ...options });
}

/** Newest first: 2.10.0 after 2.9.0, numerically per part. */
export function compareVersions(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pb[i] - pa[i];
  return 0;
}

/** The release tags that carry the site, newest first; older releases have none and are left out. */
export function listReleaseTags(repoRoot = REPO_ROOT) {
  const tags = git(repoRoot, ["tag", "--list", "v*"])
    .split("\n")
    .filter((tag) => RELEASE_TAG.test(tag));
  const withSite = tags.filter((tag) => {
    try {
      git(repoRoot, ["cat-file", "-e", `${tag}:site/pages/index.html`], { stdio: "pipe" });
      return true;
    } catch {
      return false;
    }
  });
  return withSite.sort((a, b) => compareVersions(a.slice(1), b.slice(1)));
}

/** Extracts the release paths of a tag into destDir; a path the tag lacks is skipped, the build then says what is missing. */
export function extractTag(repoRoot, tag, destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  const present = RELEASE_PATHS.filter((p) => {
    try {
      git(repoRoot, ["cat-file", "-e", `${tag}:${p}`], { stdio: "pipe" });
      return true;
    } catch {
      return false;
    }
  });
  const archive = execFileSync("git", ["archive", "--format=tar", tag, "--", ...present], {
    cwd: repoRoot,
    maxBuffer: 1024 * 1024 * 1024,
  });
  execFileSync("tar", ["-x", "-C", destDir], { input: archive });
}

/**
 * Which versions to build and from where: every release from its tag; the
 * working tree replaces the tag of its own version, or is the newest entry
 * when it is ahead of every tag. Newest first; the first entry is the latest.
 */
export function planVersions({ tags, workingVersion }) {
  const entries = tags
    .filter((tag) => tag !== `v${workingVersion}`)
    .map((tag) => ({ version: tag.slice(1), tag, source: "tag" }));
  entries.push({
    version: workingVersion,
    tag: tags.includes(`v${workingVersion}`) ? `v${workingVersion}` : null,
    source: "worktree",
  });
  return entries
    .sort((a, b) => compareVersions(a.version, b.version))
    .map((entry, i) => ({ ...entry, latest: i === 0 }));
}

/** Builds all versions into outDir. */
export function buildAll({ repoRoot = REPO_ROOT, outDir = path.join(REPO_ROOT, "site-dist"), base = "/" } = {}) {
  if (!base.startsWith("/") || !base.endsWith("/")) {
    throw new Error(`--base must start and end with "/": got "${base}"`);
  }
  const workingVersion = readVersion(fs.readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8"));
  const plan = planVersions({ tags: listReleaseTags(repoRoot), workingVersion });
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "pixpixpix-site-releases-"));
  try {
    for (const entry of plan) {
      entry.root = entry.source === "tag" ? path.join(scratch, entry.tag) : repoRoot;
      if (entry.source === "tag") extractTag(repoRoot, entry.tag, entry.root);
      entry.srcDir = path.join(entry.root, "site");
      try {
        entry.pages = collectPages({
          srcDir: entry.srcDir,
          generated: generatedPages(entry.root),
          news: latestNews(entry.version, entry.root),
        });
      } catch (err) {
        throw new Error(`${entry.tag ?? "working tree"}: ${err.message}`, { cause: err });
      }
    }
    const versions = plan.map((entry) => ({
      version: entry.version,
      base: entry.latest ? base : `${base}v${entry.version}/`,
      pages: entry.pages.map((page) => page.url),
      latest: entry.latest,
    }));
    const knownTargets = new Set(versions.flatMap((entry) => entry.pages.map((page) => `${entry.base}${page}`)));
    const assetsDir = path.join(repoRoot, "site/assets");
    const builds = [];
    // The root first: it wipes outDir, the version folders are written into it after.
    for (const entry of plan) {
      const targets = entry.latest ? [["", base]] : [];
      targets.push([`v${entry.version}`, `${base}v${entry.version}/`]);
      for (const [folder, versionBase] of targets) {
        buildSite({
          srcDir: entry.srcDir,
          outDir: path.join(outDir, folder),
          version: entry.version,
          base: versionBase,
          assetsDir,
          pages: entry.pages,
          versions,
          knownTargets,
        });
        builds.push({ version: entry.version, path: folder ? `${folder}/` : "" });
      }
    }
    fs.writeFileSync(
      path.join(outDir, "versions.json"),
      JSON.stringify(
        plan.map((entry) => ({ version: entry.version, tag: entry.tag, path: `v${entry.version}/`, latest: entry.latest })),
        null,
        2,
      ),
    );
    return { outDir, builds, versions: plan.map((entry) => entry.version) };
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--out") opts.outDir = path.resolve(argv[++i]);
    else if (arg === "--base") opts.base = argv[++i];
    else throw new Error(`Unknown argument: ${arg}`);
  }
  return opts;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const result = buildAll(parseArgs(process.argv.slice(2)));
    console.log(`Built ${result.versions.map((v) => `v${v}`).join(", ")} into ${result.outDir}`);
  } catch (err) {
    console.error(`build-all: ${err.message}`);
    process.exit(1);
  }
}
