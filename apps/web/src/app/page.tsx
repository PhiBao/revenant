"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  createJob,
  listExamples,
  type Certificate as Cert,
  type Example,
  type Failure,
  type Job,
} from "@/lib/api";
import { Certificate } from "@/components/Certificate";
import { Curtain } from "@/components/Curtain";
import { DeathCertificate } from "@/components/DeathCertificate";
import { Ekg } from "@/components/Ekg";
import { Ledger } from "@/components/Ledger";
import { Mark } from "@/components/Mark";
import { Vitals } from "@/components/Vitals";

const REPO = "https://github.com/PhiBao/revenant";

type Phase = "idle" | "loading" | "autopsy" | "failed";

/** The one-sentence finding shown above the ledger. Built only from the certificate. */
function finding(cert: Cert, date: string): string {
  const total = cert.files.length;
  const survive = cert.files.filter((f) => f.verdict !== "MISSING" && f.verdict !== "UNKNOWN").length;
  const later = cert.files.filter((f) => f.verdict === "ANACHRONISM").length;
  const parts = [`${survive} of the ${total} files this page uses survive in the archive.`];
  if (later > 0) {
    parts.push(`${later} of them were first captured after ${date}, so they may not match the page as it was that day.`);
  }
  const first = cert.files[0];
  if (first?.verdict === "UNCERTAIN") {
    parts.push("The page itself changed around that date, so its exact text on the day cannot be proven.");
  }
  return parts.join(" ");
}

function scrollToRef(el: HTMLElement | null) {
  if (!el) return;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
  el.focus({ preventScroll: true });
}

export default function Home() {
  const [examples, setExamples] = useState<Example[] | null>(null);
  const [loadFailure, setLoadFailure] = useState<Failure | null>(null);
  const [exampleId, setExampleId] = useState("");
  const [date, setDate] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState({ strict: false, nearest: false });
  const [strict, setStrict] = useState<Job | null>(null);
  const [nearest, setNearest] = useState<Job | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [revealed, setRevealed] = useState(false);

  const autopsyRef = useRef<HTMLHeadingElement>(null);
  const revealRef = useRef<HTMLHeadingElement>(null);
  const failRef = useRef<HTMLDivElement>(null);

  const example = examples?.find((e) => e.id === exampleId) ?? null;

  const fetchExamples = useCallback(() => {
    listExamples()
      .then((list) => {
        setExamples(list);
        const first = list[0];
        if (first) {
          setExampleId(first.id);
          setDate(first.default_date);
        }
      })
      .catch((f: Failure) => setLoadFailure(f));
  }, []);

  useEffect(() => {
    fetchExamples();
  }, [fetchExamples]);

  const retryExamples = () => {
    setLoadFailure(null);
    fetchExamples();
  };

  const begin = useCallback(
    async (chosenDate?: string) => {
      if (!example) return;
      const day = chosenDate ?? date;
      setDate(day);
      setPhase("loading");
      setFailure(null);
      setRevealed(false);
      setStrict(null);
      setNearest(null);
      setProgress({ strict: false, nearest: false });

      const strictP = createJob(example.id, day, "strict").then((j) => {
        setProgress((p) => ({ ...p, strict: true }));
        return j;
      });
      const nearestP = createJob(example.id, day, "include-anachronisms").then((j) => {
        setProgress((p) => ({ ...p, nearest: true }));
        return j;
      });
      const [s, n] = await Promise.allSettled([strictP, nearestP]);

      if (s.status === "fulfilled" && n.status === "fulfilled") {
        setStrict(s.value);
        setNearest(n.value);
        setPhase("autopsy");
        return;
      }
      const reason = s.status === "rejected" ? s.reason : n.status === "rejected" ? n.reason : null;
      setFailure((reason as Failure) ?? { kind: "network" });
      setPhase("failed");
    },
    [example, date],
  );

  useEffect(() => {
    if (phase === "autopsy") scrollToRef(autopsyRef.current);
    if (phase === "failed") scrollToRef(failRef.current);
  }, [phase]);

  useEffect(() => {
    if (revealed) scrollToRef(revealRef.current);
  }, [revealed]);

  const reset = () => {
    setPhase("idle");
    setStrict(null);
    setNearest(null);
    setFailure(null);
    setRevealed(false);
    window.scrollTo({ top: 0 });
  };

  const busy = phase === "loading";
  const later =
    nearest?.certificate.files.filter((f) => f.included && f.verdict === "ANACHRONISM").length ?? 0;

  return (
    <main className="page">
      <header className="masthead">
        <span className="brand">
          <Mark />
          <span className="wordmark">Revenant</span>
        </span>
        <nav aria-label="Project">
          <a href={REPO} target="_blank" rel="noreferrer">
            Source
          </a>
          <a href={`${REPO}/blob/main/KIRO_USAGE.md`} target="_blank" rel="noreferrer">
            Built with Kiro
          </a>
        </nav>
      </header>

      <section className="hero">
        <h1>
          Bring a dead website back <em>as it was</em>, and see which parts are true.
        </h1>
        <p className="lede">
          Pick a day. Revenant rebuilds the page from the public web archive and stamps every file
          with when it was captured and whether it can be trusted for that day.
        </p>
        <Ekg alive={revealed} label={revealed ? "A steady heartbeat" : "A flat line"} />
      </section>

      {/* ACT I */}
      <section className="act" aria-labelledby="act1">
        <div className="act-mark">
          <span>I</span>
          <h2 id="act1">The slab</h2>
        </div>

        {loadFailure ? (
          <DeathCertificate
            failure={loadFailure}
            example={null}
            date=""
            onPickDate={() => {}}
            onRetry={retryExamples}
            onReset={reset}
          />
        ) : !example ? (
          <p className="fine" aria-live="polite">
            Opening the archive room…
          </p>
        ) : (
          <form
            className="slab"
            onSubmit={(e) => {
              e.preventDefault();
              void begin();
            }}
          >
            <div className="specimen">
              <p className="specimen-no">Specimen</p>
              <h3 className="specimen-title">{example.title}</h3>
              <p className="specimen-blurb">{example.blurb}</p>
              <p className="specimen-rights">
                <span>Rights</span> {example.rights}
              </p>
            </div>

            <div className="field">
              <label htmlFor="date">Restore it as it was on</label>
              <input
                id="date"
                type="date"
                value={date}
                min="1996-01-01"
                max="2001-12-31"
                onChange={(e) => setDate(e.target.value)}
                required
                disabled={busy}
              />
              <div className="chips" role="group" aria-label="Suggested dates">
                {[{ date: example.default_date, label: "Default" }, ...example.alternate_dates].map((a) => (
                  <button
                    key={a.date}
                    type="button"
                    className="chip"
                    aria-pressed={date === a.date}
                    disabled={busy}
                    onClick={() => setDate(a.date)}
                  >
                    {a.date} · {a.label}
                  </button>
                ))}
              </div>
            </div>

            {busy ? (
              <ol className="progress" aria-live="polite">
                <li className={progress.strict ? "done" : ""}>
                  Reading the archive and classifying every file
                </li>
                <li className={progress.nearest ? "done" : ""}>Preparing the comparison restore</li>
              </ol>
            ) : (
              <button type="submit" className="btn btn-primary">
                Begin autopsy
              </button>
            )}
          </form>
        )}
      </section>

      {phase === "failed" && failure ? (
        <section className="act" ref={failRef} tabIndex={-1}>
          <DeathCertificate
            failure={failure}
            example={example}
            date={date}
            onPickDate={(d) => void begin(d)}
            onRetry={() => void begin()}
            onReset={reset}
          />
        </section>
      ) : null}

      {/* ACT II */}
      {phase === "autopsy" && strict && nearest ? (
        <section className="act reveal" aria-labelledby="act2">
          <div className="act-mark">
            <span>II</span>
            <h2 id="act2" ref={autopsyRef} tabIndex={-1}>
              The autopsy
            </h2>
          </div>
          <p className="report-line">
            <code>
              {strict.certificate.site}/{strict.certificate.page}
            </code>{" "}
            as of <strong>{strict.date}</strong>
            <span className="stamp stamp-inline">{strict.mode}</span>
          </p>
          <Vitals cert={strict.certificate} />
          <p className="finding">{finding(strict.certificate, strict.date)}</p>
          <Ledger cert={strict.certificate} />

          {!revealed ? (
            <div className="cta">
              <button type="button" className="btn btn-primary" onClick={() => setRevealed(true)}>
                Resurrect it
              </button>
              <p className="fine">Shows the page two ways, side by side.</p>
            </div>
          ) : null}
        </section>
      ) : null}

      {/* ACT III */}
      {phase === "autopsy" && strict && nearest && revealed ? (
        <section className="act reveal" aria-labelledby="act3">
          <div className="act-mark">
            <span>III</span>
            <h2 id="act3" ref={revealRef} tabIndex={-1}>
              The resurrection
            </h2>
          </div>
          <p className="finding">
            Left: every file taken from the capture nearest in time, whatever its date. Right: only
            what can be shown as it was on <strong>{strict.date}</strong>.
            {later > 0
              ? ` The left page looks more complete, but ${later} of its files were captured after that day.`
              : ""}
          </p>
          <Curtain
            nearestUrl={nearest.urls.preview}
            dayUrl={strict.urls.preview}
            dayLabel={`As it was on ${strict.date}`}
          />
        </section>
      ) : null}

      {/* ACT IV */}
      {phase === "autopsy" && strict && nearest && revealed ? (
        <section className="act reveal" aria-labelledby="act4">
          <div className="act-mark">
            <span>IV</span>
            <h2 id="act4">The certificate</h2>
          </div>
          <Certificate strict={strict} nearest={nearest} />
          <div className="cta">
            <button type="button" className="btn btn-ghost" onClick={reset}>
              Start over
            </button>
          </div>
        </section>
      ) : null}

      <footer className="colophon">
        <h2>What is real here</h2>
        <ul>
          <li>
            Files come from stored copies of public web-archive data, so this run is labelled{" "}
            <strong>SEEDED</strong>. Nothing is simulated: the dates, hashes and bytes are the
            archive&rsquo;s.
          </li>
          <li>
            Revenant never calls a missing file a recovered one. A page captured only as an error
            page counts as missing.
          </li>
          <li>
            The specimen is a US federal government work and in the public domain. Its provenance
            is in the repository.
          </li>
        </ul>
        <p className="fine">
          Open source under the MIT license. <a href={REPO}>{REPO.replace("https://", "")}</a>
        </p>
      </footer>
    </main>
  );
}
