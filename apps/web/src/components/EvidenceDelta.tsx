import type { Certificate } from "@/lib/api";
import { formatPercent } from "@/lib/api";

/**
 * Says what the origin server's own dates changed (W6.2). Shown only when they changed something,
 * so a page with no server dates does not get a claim it cannot back.
 */
export function EvidenceDelta({ cert }: { cert: Certificate }) {
  const lifted = cert.files.filter((f) => f.verdict_from_dates_only !== f.verdict).length;
  const before = cert.vitals_from_dates_only.fidelity_percent;
  const after = cert.vitals.fidelity_percent;
  if (lifted === 0 || before === after) return null;

  return (
    <aside className="delta" aria-label="What the server's dates changed">
      <p className="delta-title">What the server&rsquo;s own dates changed</p>
      <div className="delta-row">
        <div>
          <span className="delta-label">From capture dates alone</span>
          <span className="delta-num delta-before">{formatPercent(before)}%</span>
        </div>
        <span className="delta-arrow" aria-hidden="true">
          →
        </span>
        <div>
          <span className="delta-label">With the server&rsquo;s dates</span>
          <span className="delta-num delta-after">{formatPercent(after)}%</span>
        </div>
      </div>
      <p className="delta-body">
        The archive saved {lifted === 1 ? "one file" : `${lifted} files`} after the day you chose, so
        by capture dates alone nothing could be trusted. The server that originally held{" "}
        {lifted === 1 ? "it" : "them"} also stamped each file with the day it last changed, and the
        archive kept that stamp. Those days are before your date, so the files were already there.
        This is the server&rsquo;s claim, used as evidence. Revenant ignores it when it contradicts
        what the archive itself saw.
      </p>
    </aside>
  );
}
