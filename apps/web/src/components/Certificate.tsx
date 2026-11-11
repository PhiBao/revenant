"use client";

import { useState } from "react";
import type { Job } from "@/lib/api";
import { formatPercent, sameRestore } from "@/lib/api";

function CopyButton({ text, label }: { text: string; label: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className="btn-quiet"
      aria-label={label}
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setDone(true);
          window.setTimeout(() => setDone(false), 1600);
        } catch {
          setDone(false);
        }
      }}
    >
      {done ? "Copied" : "Copy"}
    </button>
  );
}

function filenameOf(job: Job) {
  return `revenant-${job.example}-${job.date}-${job.policy}.zip`;
}

export function Certificate({ strict, nearest }: { strict: Job; nearest: Job }) {
  const same = sameRestore(strict.certificate, nearest.certificate);
  const c = strict.certificate;
  const file = filenameOf(strict);
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const strictCount = c.files.filter((f) => f.included).length;
  const nearestCount = nearest.certificate.files.filter((f) => f.included).length;
  const verify = `sha256sum ${file}\nunzip -q ${file} -d restored\ngrep -r "web.archive.org" restored   # prints nothing`;

  return (
    <article className="paper" aria-labelledby="cert-title">
      <header className="paper-head">
        <div>
          <p className="paper-kicker">Certificate of resurrection</p>
          <h3 id="cert-title" className="paper-title">
            {c.site}/{c.page}
          </h3>
        </div>
        <p className="stamp" aria-label={`Source: ${c.mode === "SEEDED" ? "seeded fixture" : "live archive"}`}>
          {c.mode}
        </p>
      </header>

      <dl className="paper-facts">
        <div>
          <dt>As of</dt>
          <dd>{strict.date}, end of day UTC</dd>
        </div>
        <div>
          <dt>Source</dt>
          <dd>{c.archive}</dd>
        </div>
        <div>
          <dt>Completeness</dt>
          <dd>{formatPercent(c.vitals.completeness_percent)}%</dd>
        </div>
        <div>
          <dt>Fidelity</dt>
          <dd>{formatPercent(c.vitals.fidelity_percent)}%</dd>
        </div>
        <div>
          <dt>Scripts removed</dt>
          <dd>{c.scripts_removed}</dd>
        </div>
        <div>
          <dt>Links not restored</dt>
          <dd>{c.links_not_restored.length}</dd>
        </div>
      </dl>

      <div className="hashrow">
        <div>
          <p className="hash-label">SHA-256 of the as-it-was ZIP</p>
          <code className="hash">{strict.zip_sha256}</code>
        </div>
        <CopyButton text={strict.zip_sha256} label="Copy the SHA-256" />
      </div>

      <div className="downloads">
        <a className="btn" href={strict.urls.zip} download={file}>
          Download as it was
          <span className="btn-sub">{plural(strictCount, "file", "files")} · only what can be shown for that day</span>
        </a>
        {same ? null : (
          <a className="btn btn-ghost" href={nearest.urls.zip} download={filenameOf(nearest)}>
            Download what archive playback gives
            <span className="btn-sub">
              {plural(nearestCount, "file", "files")} · includes files Revenant cannot vouch for
            </span>
          </a>
        )}
      </div>

      <div className="verify">
        <div className="verify-head">
          <p className="hash-label">Check it yourself</p>
          <CopyButton text={verify} label="Copy the verification commands" />
        </div>
        <pre className="code">{verify}</pre>
        <p className="fine">
          The first line must print the hash above. The last line must print nothing. Every file in the
          ZIP is listed with its capture time in <code>REVENANT-CERTIFICATE.json</code>.
        </p>
      </div>

      <footer className="paper-foot">
        <a href={strict.urls.preview} target="_blank" rel="noreferrer">
          Open the restored page
        </a>
        <a href={`${strict.urls.preview}REVENANT-CERTIFICATE.json`} target="_blank" rel="noreferrer">
          Read the certificate JSON
        </a>
        <span className="fine">Kept for {Math.round(strict.expires_in_secs / 60)} minutes</span>
      </footer>
    </article>
  );
}
