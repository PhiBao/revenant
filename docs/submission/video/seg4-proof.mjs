import fs from "node:fs";
import { launch, openContext, closeAndGetVideo, warm, card, terminal, reveal, EKG_SVG, sleep } from "./lib.mjs";

const read = (f) => fs.readFileSync(f, "utf8").trimEnd().split("\n");
const tint = (l) => (l.startsWith("$") ? ["cmd", l] : l.startsWith("#") || l.startsWith("(") ? ["note", l] : ["out", l]);

const browser = await launch();
const s = await openContext(browser, "proof");
await warm(s.page);

// Verify the download, exactly as the certificate says to.
await terminal(s.page, "bash: check the download yourself", read("/tmp/video/t1-verify.txt").map(tint), { stepMs: 700, holdMs: 3200 });

// The security finding, found by the read-only reviewer agent and fixed spec-first.
await terminal(
  s.page,
  "bash: the adversarial review found a real bug",
  read("/tmp/video/t3-security.txt").map((l) =>
    l.startsWith("accepted=") ? ["good", l] : l.startsWith("$") ? ["cmd", l] : l.startsWith("#") ? ["note", l] : ["out", l],
  ),
  { stepMs: 750, holdMs: 3200 },
);

// The hook that stops a credential from being committed.
await terminal(
  s.page,
  "bash: a PreToolUse hook guards every commit",
  read("/tmp/video/t2-hooks.txt").map((l) =>
    l.startsWith("Blocked") || l.startsWith("hook exit") ? ["bad", l] : l.startsWith("$") ? ["cmd", l] : l.startsWith("#") ? ["note", l] : ["out", l],
  ),
  { stepMs: 450, holdMs: 3000 },
);

// What was built with Kiro, with counts taken from the repository.
await card(
  s.page,
  `<h1>Built spec first, with Kiro.</h1>
   <ul class="k">
     <li><b>3 specs</b>requirements written before code. Real data and a security review changed them four times, each logged in the spec.</li>
     <li><b>5 steering files</b>product, tech, structure, security, testing.</li>
     <li><b>5 hooks</b>tests on save, fixtures re-check, secret guard on commit, checks on stop.</li>
     <li><b>2 custom agents</b>a spec-first Rust agent and a read-only adversarial reviewer.</li>
     <li><b>11 property tests</b>order invariance, no false anachronism, path safety, no panics.</li>
     <li><b>128 tests</b>one script runs them all, plus a live end-to-end check.</li>
   </ul>`,
  300,
);
await reveal(s.page, "ul.k li", 900);
await sleep(4200);

await card(
  s.page,
  `<h1>Your website died.<br><em>Revenant tells you what is true.</em></h1>
   <p>Resurrection · open source, MIT · github.com/PhiBao/revenant</p>${EKG_SVG}`,
  6500,
);

const file = await closeAndGetVideo(s);
fs.copyFileSync(file, "/tmp/video/seg4.webm");
await browser.close();
console.log("seg4", s.at().toFixed(1) + "s");
