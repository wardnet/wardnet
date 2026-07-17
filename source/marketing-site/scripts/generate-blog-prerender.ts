#!/usr/bin/env node
/**
 * Build-time blog prerender.
 *
 * The site is a client-rendered SPA served from a single static index.html, so
 * crawlers and social scrapers (Mastodon, Reddit, Lemmy, Discord, Google) only
 * ever see one generic <title>/description — every /blog link would preview as
 * the homepage blurb. This step fixes that without introducing SSR: after
 * `vite build`, it copies the built dist/index.html for each blog route and
 * rewrites the <head> with per-page <title>, description, canonical, and Open
 * Graph / Twitter card tags.
 *
 * Output (served by GitHub Pages for the matching path):
 *   dist/blog/index.html                — the blog index
 *   dist/blog/<slug>/index.html         — one per post in content/blog.yml
 *
 * The bundled <script> tag is preserved untouched, so a browser still boots the
 * SPA and client-routes to the same page; only the crawler-visible <head>
 * differs. Runs after `vite build` in the `build` script — it is a dist
 * post-process and is not imported by app code.
 *
 * Degrades gracefully: if dist/index.html is missing (prerender run without a
 * prior build) it logs and exits 0 rather than failing the pipeline.
 */

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { parse } from "yaml";

const ROOT = resolve(fileURLToPath(import.meta.url), "../..");
const SITE = "https://wardnet.network";
/** Fallback social image when a post declares no `image`. */
const DEFAULT_OG_IMAGE = "/docs/device-routing/routing-edit.png";

interface Post {
  slug: string;
  title: string;
  description: string;
  image?: string;
}

interface PageMeta {
  title: string;
  description: string;
  url: string;
  image: string;
  type: "website" | "article";
}

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Build the replacement <head> tags for one page. */
function headTags(meta: PageMeta): string {
  const image = meta.image.startsWith("http") ? meta.image : `${SITE}${meta.image}`;
  const t = escapeAttr(meta.title);
  const d = escapeAttr(meta.description);
  const u = escapeAttr(meta.url);
  const i = escapeAttr(image);
  return [
    `<title>${t}</title>`,
    `<meta name="description" content="${d}" />`,
    `<link rel="canonical" href="${u}" />`,
    `<meta property="og:type" content="${meta.type}" />`,
    `<meta property="og:site_name" content="Wardnet" />`,
    `<meta property="og:title" content="${t}" />`,
    `<meta property="og:description" content="${d}" />`,
    `<meta property="og:url" content="${u}" />`,
    `<meta property="og:image" content="${i}" />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${t}" />`,
    `<meta name="twitter:description" content="${d}" />`,
    `<meta name="twitter:image" content="${i}" />`,
  ].join("\n    ");
}

/** Write dist/<route>/index.html from the template with a rewritten <head>. */
async function emit(template: string, route: string, meta: PageMeta): Promise<void> {
  const html = template
    .replace(/<title>[\s\S]*?<\/title>/, "")
    .replace(/<meta[^>]*name="description"[^>]*\/>/, "")
    .replace("</head>", `    ${headTags(meta)}\n  </head>`);
  // `dir`/`route` are build-time constants (a fixed ROOT plus route names from
  // content/blog.yml), never user input — the non-literal-path findings below
  // are false positives for a build script.
  const dir = resolve(ROOT, "dist", route);
  // eslint-disable-next-line security/detect-non-literal-fs-filename
  await mkdir(dir, { recursive: true });
  // eslint-disable-next-line security/detect-non-literal-fs-filename
  await writeFile(resolve(dir, "index.html"), html, "utf8");
}

async function main(): Promise<void> {
  let template: string;
  try {
    template = await readFile(resolve(ROOT, "dist/index.html"), "utf8");
  } catch {
    console.warn("blog-prerender: dist/index.html not found — run after `vite build`. Skipping.");
    return;
  }

  const raw = await readFile(resolve(ROOT, "content/blog.yml"), "utf8");
  const posts = ((parse(raw) as { posts?: Post[] }).posts ?? []).filter((p) => p.slug);

  await emit(template, "blog", {
    title: "Blog — Wardnet",
    description: "Notes on why Wardnet exists and how it works.",
    url: `${SITE}/blog`,
    image: DEFAULT_OG_IMAGE,
    type: "website",
  });

  for (const post of posts) {
    await emit(template, `blog/${post.slug}`, {
      title: `${post.title} — Wardnet`,
      description: post.description.trim(),
      url: `${SITE}/blog/${post.slug}`,
      image: post.image ?? DEFAULT_OG_IMAGE,
      type: "article",
    });
  }

  console.log(`blog-prerender: wrote ${posts.length + 1} prerendered page(s).`);
}

await main();
