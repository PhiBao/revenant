import type { Verdict } from "@/lib/api";
import { VERDICTS } from "@/lib/verdict";

/** Verdict as shape, word and color together (W5.1). */
export function VerdictTag({ verdict }: { verdict: Verdict }) {
  const meta = VERDICTS[verdict];
  return (
    <span className={`tag tag-${verdict.toLowerCase()}`} title={meta.meaning}>
      <span aria-hidden="true" className="tag-glyph">
        {meta.glyph}
      </span>
      {meta.label}
    </span>
  );
}
