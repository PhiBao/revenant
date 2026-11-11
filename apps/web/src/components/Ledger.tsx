import type { Certificate } from "@/lib/api";
import { formatCapture, formatDay, formatDelta } from "@/lib/api";
import { VERDICT_ORDER, VERDICTS } from "@/lib/verdict";
import { VerdictTag } from "./VerdictTag";

const sentence = (t: string) => (t ? t.charAt(0).toUpperCase() + t.slice(1) : t);
const ended = (t: string) => (/[.!?]$/.test(t) ? t : `${t}.`);

/** One plain line saying why a file got its verdict, and what server evidence changed (W6.7). */
function whyText(f: Certificate["files"][number]): string {
  const parts: string[] = [];
  if (f.verdict === "ATTESTED" && f.origin_modified) {
    parts.push(`the server says it last changed on ${formatDay(f.origin_modified)}`);
  } else {
    parts.push(f.basis);
  }
  if (f.verdict_from_dates_only !== f.verdict) {
    parts.push(`capture dates alone said ${VERDICTS[f.verdict_from_dates_only].label.toLowerCase()}`);
  }
  const text = parts.map((p) => ended(sentence(p))).join(" ");
  return f.note ? `${text} ${ended(sentence(f.note))}` : text;
}

export function Ledger({ cert }: { cert: Certificate }) {
  const counts = VERDICT_ORDER.map((v) => ({
    verdict: v,
    n: cert.files.filter((f) => f.verdict === v).length,
  })).filter((c) => c.n > 0);

  return (
    <div className="ledger-wrap">
      <ul className="tally" aria-label="Files by verdict">
        {counts.map((c) => (
          <li key={c.verdict}>
            <span className="tally-n">{c.n}</span>
            <VerdictTag verdict={c.verdict} />
          </li>
        ))}
      </ul>

      <table className="ledger">
        <caption className="sr-only">
          Every file the page uses, with its verdict at {cert.target_time.slice(0, 8)}
        </caption>
        <thead>
          <tr>
            <th scope="col">File</th>
            <th scope="col">Verdict</th>
            <th scope="col" className="col-capture">
              Captured
            </th>
            <th scope="col" className="col-delta">
              From the day
            </th>
          </tr>
        </thead>
        <tbody>
          {cert.files.map((f, i) => (
            <tr
              key={f.path}
              className={`row row-${f.verdict.toLowerCase()}`}
              style={{ ["--i" as string]: i }}
            >
              <th scope="row" className="cell-path">
                {f.path}
              </th>
              <td>
                <VerdictTag verdict={f.verdict} />
                <span className="why">{whyText(f)}</span>
              </td>
              <td className="col-capture">{formatCapture(f.capture_time)}</td>
              <td className="col-delta">{formatDelta(f.delta_days)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <details className="legend">
        <summary>What the verdicts mean</summary>
        <dl>
          {VERDICT_ORDER.map((v) => (
            <div key={v}>
              <dt>
                <VerdictTag verdict={v} />
              </dt>
              <dd>{VERDICTS[v].meaning}</dd>
            </div>
          ))}
        </dl>
      </details>
    </div>
  );
}
