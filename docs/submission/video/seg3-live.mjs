import fs from "node:fs";
import { launch, openContext, closeAndGetVideo, caption, sleep, BASE } from "./lib.mjs";

const browser = await launch();
const s = await openContext(browser, "live");
const { page } = s;
await page.goto(BASE + "/", { waitUntil: "networkidle" });

await caption(page, "Now a page of your own, asked live. Revenant never visits the address. It only asks the archive what it holds.");
await page.getByRole("button", { name: /A page of your own/ }).click();
await sleep(1500);
await page.getByRole("button", { name: "Try the Y2K.gov page, live" }).click();
await sleep(1800);
await page.getByRole("checkbox").check();
await sleep(2200);
await page.getByRole("button", { name: "Begin autopsy" }).click();
await page.locator(".live-progress").waitFor();
const tProgressStart = s.at();
await caption(page, "One question at a time, on purpose, so the archive stays polite.");

await page.getByRole("heading", { name: "The autopsy" }).waitFor({ timeout: 240000 });
const tProgressEnd = s.at();
await sleep(1500);
const vitals = await page.locator(".vital-value").allInnerTexts();
const unknownNote = await page.locator(".finding").innerText();
const matches = vitals[0] === "58.3%" && vitals[1] === "0.0%";
console.log("live vitals:", vitals.join(" / "), "| matches stored:", matches);
await caption(
  page,
  matches
    ? "Same page, same date, now labelled LIVE. The archive's answers give the same scores as the stored copy."
    : `Labelled LIVE: ${vitals[0]} complete, ${vitals[1]} fidelity. ${/did not answer/.test(unknownNote) ? "A file the archive would not answer for is shown as unknown, not lost." : ""}`,
);
await sleep(5200);
await caption(page, "");

const file = await closeAndGetVideo(s);
fs.copyFileSync(file, "/tmp/video/seg3.webm");
fs.writeFileSync("/tmp/video/seg3.times.json", JSON.stringify({ tProgressStart, tProgressEnd }));
await browser.close();
console.log("seg3", s.at().toFixed(1) + "s", "progress", tProgressStart.toFixed(1), "->", tProgressEnd.toFixed(1));
