import fs from "node:fs";
import { launch, openContext, closeAndGetVideo, warm, card, EKG_SVG, sleep } from "./lib.mjs";

const browser = await launch();
const s = await openContext(browser, "intro");
await warm(s.page);

await card(
  s.page,
  `<h1>Every restore tool gets you files.<br><em>Revenant tells you which ones are true.</em></h1>${EKG_SVG}`,
  6000,
);

await card(
  s.page,
  `<h1>A dead website. A copy in the archive.<br><em>But which parts are real?</em></h1>
   <p>Each file on a page is saved at a different moment. Archive playback quietly mixes them, and shows a page that never existed in that form.</p>
   <p>One study found at most 1 archived page in 5 was both complete and consistent in time.</p>
   <p class="src">Ainsworth, Nelson and Van de Sompel. “Only One Out of Five Archived Web Pages Existed as Presented.” ACM Hypertext 2015.</p>`,
  11500,
);

const file = await closeAndGetVideo(s);
fs.copyFileSync(file, "/tmp/video/seg1.webm");
await browser.close();
console.log("seg1", s.at().toFixed(1) + "s");
