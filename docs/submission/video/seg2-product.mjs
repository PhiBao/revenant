import fs from "node:fs";
import { launch, openContext, closeAndGetVideo, caption, sleep, scrollTo, BASE } from "./lib.mjs";

const browser = await launch();
const s = await openContext(browser, "product");
const { page } = s;
await page.goto(BASE + "/", { waitUntil: "networkidle" });

await caption(page, "Pick a page and a day. This specimen is part of Y2K.gov, a US government site whose domain no longer resolves.");
await scrollTo(page, "section:has(#act1)", 10);
await sleep(5200);

await page.getByRole("button", { name: "Begin autopsy" }).click();
await page.getByRole("heading", { name: "The autopsy" }).waitFor();
await sleep(1500);
await caption(page, "Completeness 58.3%: 7 of 12 files survive. Fidelity 0.0%: none can be shown to date from before the day.");
await sleep(5500);

await scrollTo(page, ".tally", 20);
await sleep(900);
await caption(page, "Six images were first captured on March 4, after the day chosen. Five files were never archived: they count as missing.");
await sleep(6200);

await page.getByRole("button", { name: "Resurrect it" }).click();
await page.getByRole("heading", { name: "The certificate" }).waitFor();
await sleep(500);
await scrollTo(page, ".curtain-labels", 14);
await sleep(1300);
await caption(page, "Left: every file from the nearest capture, whatever its date. Right: only what can be shown as it was on 2000-03-02.");
await sleep(4200);

// Drag the divider across so the difference is visible.
const slider = page.locator(".curtain-slider");
for (const [from, to] of [[50, 94], [94, 6], [6, 50]]) {
  for (let i = 0; i <= 22; i++) {
    await slider.fill(String(Math.round(from + ((to - from) * i) / 22)));
    await sleep(65);
  }
  await sleep(700);
}
await caption(page, "The left page looks more complete. Six of its files were captured after that day.");
await sleep(3800);

await scrollTo(page, ".paper", 14);
await sleep(1200);
await caption(page, "Every restore gets a certificate: each file's capture time, and a SHA-256 for the ZIP.");
await sleep(4600);
await scrollTo(page, ".verify", 40);
await sleep(900);
await caption(page, "And the commands to check it yourself.");
await sleep(3800);
await caption(page, "");

const file = await closeAndGetVideo(s);
fs.copyFileSync(file, "/tmp/video/seg2.webm");
await browser.close();
console.log("seg2", s.at().toFixed(1) + "s");
