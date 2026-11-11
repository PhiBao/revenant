import fs from "node:fs";
import { chromium } from "playwright-core";

export const BASE = process.env.BASE ?? "http://127.0.0.1:18087";
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function openContext(browser, name) {
  const dir = `/tmp/video/raw-${name}`;
  fs.rmSync(dir, { recursive: true, force: true });
  const ctx = await browser.newContext({
    viewport: { width: 1280, height: 720 },
    recordVideo: { dir, size: { width: 1280, height: 720 } },
    reducedMotion: "no-preference",
  });
  const page = await ctx.newPage();
  const started = Date.now();
  return { ctx, page, dir, started, at: () => (Date.now() - started) / 1000 };
}

export async function closeAndGetVideo(session) {
  const video = session.page.video();
  await session.ctx.close();
  return await video.path();
}

/** Load the app once so its fonts and design variables are available to every card. */
export async function warm(page) {
  await page.goto(BASE + "/", { waitUntil: "networkidle" });
}

export async function caption(page, text) {
  await page.evaluate((t) => {
    let c = document.getElementById("cap");
    if (!c) {
      c = document.createElement("div");
      c.id = "cap";
      c.style.cssText =
        'position:fixed;left:0;right:0;bottom:0;padding:15px 40px 17px;background:#0b0c0a;' +
        'border-top:1px solid #3c3f35;color:#e8e3d2;font:500 21px/1.4 "IBM Plex Mono",monospace;z-index:99999;';
      document.body.appendChild(c);
    }
    c.textContent = t;
    c.style.display = t ? "block" : "none";
    document.body.style.paddingBottom = t ? "120px" : "";
  }, text);
}

const CARD_CSS = `
  html,body{margin:0;height:100%;background:#0b0c0a;color:#e8e3d2}
  .card{height:100vh;box-sizing:border-box;padding:0 90px;display:flex;flex-direction:column;justify-content:center}
  .card h1{font-family:"Fraunces Variable",Georgia,serif;font-weight:400;font-size:64px;line-height:1.08;margin:0;letter-spacing:-.015em;font-variation-settings:"opsz" 144;max-width:17em}
  .card h1 em{color:#a49d87}
  .card p{font:400 24px/1.55 "IBM Plex Mono",monospace;color:#a49d87;max-width:44em;margin:26px 0 0}
  .card .src{font-size:17px;color:#6f6a58}
  .big{font-family:"Fraunces Variable",Georgia,serif;font-size:96px;line-height:1;font-variation-settings:"opsz" 144;margin:0}
  .rule{height:1px;background:#2a2c25;margin:34px 0}
  .ekg{width:100%;height:70px;margin-top:40px}
  .ekg line{stroke:#3c3f35;stroke-width:1.5}
  .ekg path{fill:none;stroke:#7ee0a1;stroke-width:2.4;stroke-linecap:round;stroke-linejoin:round;filter:drop-shadow(0 0 7px rgba(126,224,161,.6));
    stroke-dasharray:1;stroke-dashoffset:1;animation:beat 2.4s .6s cubic-bezier(.3,.6,.2,1) forwards}
  @keyframes beat{to{stroke-dashoffset:0}}
  .win{margin:0 70px;border:1px solid #3c3f35;background:#0e100c;box-shadow:0 0 0 6px #0b0c0a,0 0 0 7px #2a2c25}
  .win .bar{padding:10px 18px;border-bottom:1px solid #2a2c25;font:400 15px "IBM Plex Mono",monospace;color:#6f6a58}
  .win pre{margin:0;padding:24px 28px;font:400 19px/1.55 "IBM Plex Mono",monospace;color:#e8e3d2;white-space:pre-wrap;word-break:break-all;min-height:300px}
  .win pre .l{opacity:0}
  .win pre .l.on{opacity:1}
  .cmd{color:#e8e3d2}.out{color:#a49d87}.bad{color:#e0805a}.good{color:#7ee0a1}.note{color:#6f6a58}
  ul.k{list-style:none;padding:0;margin:30px 0 0;display:grid;grid-template-columns:1fr 1fr;gap:20px 50px}
  ul.k li{font:400 21px/1.45 "IBM Plex Mono",monospace;color:#a49d87;opacity:0;transition:opacity .5s}
  ul.k li.on{opacity:1}
  ul.k b{display:block;color:#e8e3d2;font-weight:500;margin-bottom:2px}
`;

export async function card(page, html, holdMs) {
  await page.evaluate(
    ([css, h]) => {
      document.head.insertAdjacentHTML("beforeend", `<style id="cardcss">${css}</style>`);
      document.body.innerHTML = `<div class="card">${h}</div>`;
    },
    [CARD_CSS, html],
  );
  await sleep(holdMs);
}

/** A terminal window whose lines appear one at a time, so a viewer can read them. */
export async function terminal(page, title, lines, { stepMs = 650, holdMs = 4000 } = {}) {
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const body = lines
    .map(([cls, text]) => `<span class="l ${cls}">${esc(text)}\n</span>`)
    .join("");
  await page.evaluate(
    ([css, t, b]) => {
      if (!document.getElementById("cardcss")) document.head.insertAdjacentHTML("beforeend", `<style id="cardcss">${css}</style>`);
      document.body.innerHTML = `<div class="card" style="padding:0"><div class="win"><div class="bar">${t}</div><pre>${b}</pre></div></div>`;
    },
    [CARD_CSS, title, body],
  );
  const n = lines.length;
  for (let i = 0; i < n; i++) {
    await page.evaluate((i) => document.querySelectorAll(".l")[i]?.classList.add("on"), i);
    await sleep(stepMs);
  }
  await sleep(holdMs);
}

export async function reveal(page, selector, stepMs) {
  const n = await page.locator(selector).count();
  for (let i = 0; i < n; i++) {
    await page.evaluate(([s, i]) => document.querySelectorAll(s)[i]?.classList.add("on"), [selector, i]);
    await sleep(stepMs);
  }
}

export const EKG_SVG = `<svg class="ekg" viewBox="0 0 1100 70" preserveAspectRatio="none"><line x1="0" y1="35" x2="1100" y2="35"/><path pathLength="1" d="M0 35H430l14-6 12 6h24l12-30 16 62 14-46 12 16h26l14-8 12 8H1100"/></svg>`;

export async function launch() {
  return await chromium.launch({ executablePath: "/home/kiter/.local/bin/chromium", args: ["--no-sandbox"] });
}

/** Scroll so an element's top sits `offset` px below the viewport top. */
export async function scrollTo(page, selector, offset = 24) {
  await page.evaluate(([sel, off]) => {
    const el = document.querySelector(sel);
    if (el) window.scrollTo({ top: el.getBoundingClientRect().top + window.scrollY - off, behavior: "smooth" });
  }, [selector, offset]);
}
