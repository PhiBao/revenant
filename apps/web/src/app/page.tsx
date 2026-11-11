"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  createJob,
  createLive,
  getLive,
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
const SAMPLE_LIVE = { url: "http://www.y2k.gov/briefings.html", date: "2000-03-02" };

type Phase = "idle" | "loading" | "autopsy" | "failed";
type Source = "specimen" | "live";

/** The one-sentence finding shown above the ledger. Built only from the certificate. */
function finding(cert: Cert, date: string): string {
  const total = cert.files.length;
  const survive = cert.files.filter((f) => f.verdict !== "MISSING" && f.verdict !== "UNKNOWN").length;
  const later = cert.files.filter((f) => f.verdict === "ANACHRONISM").length;
  const unknown = cert.files.filter((f) => f.verdict === "UNKNOWN").length;
  const parts = [`${survive} of the ${total} files this page uses survive in the archive.`];
  if (later > 0) {
    parts.push(`${later} of them were first captured after ${date}, so they may not match the page as it was that day.`);
  }
  if (unknown > 0) {
    parts.push(
      `The archive did not answer for ${unknown} ${unknown === 1 ? "file" : "files"}, so ${unknown === 1 ? "it is" : "they are"} left out of the score instead of counted as lost.`,
    );
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

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export default function Home() {
  const [examples, setExamples] = useState<Example[] | null>(null);
  const [loadFailure, setLoadFailure] = useState<Failure | null>(null);
  const [source, setSource] = useState<Source>("specimen");
  const [exampleId, setExampleId] = useState("");
  const [date, setDate] = useState("");
  const [liveUrl, setLiveUrl] = useState("");
  const [liveDate, setLiveDate] = useState("");
  const [rights, setRights] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState({ strict: false, nearest: false });
  const [liveStage, setLiveStage] = useState("");
  const [liveRequests, setLiveRequests] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [strict, setStrict] = useState<Job | null>(null);
  const [nearest, setNearest] = useState<Job | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [revealed, setRevealed] = useState(false);

  const autopsyRef = useRef<HTMLHeadingElement>(null);
  const revealRef = useRef<HTMLHeadingElement>(null);
  const failRef = useRef<HTMLDivElement>(null);
  /** Bumped to cancel a running poll when the visitor starts over. */
  const pollToken = useRef(0);

  const example = examples?.find((e) => e.id === exampleId) ?? null;
  const live = source === "live";

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

  // Seconds since a live run began, shown so a long wait never looks like a hang.
  useEffect(() => {
    if (phase !== "loading" || !live) return;
    const started = Date.now();
    const timer = window.setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [phase, live]);

  const succeed = useCallback((s: Job, n: Job, runNotes: string[] = []) => {
    setStrict(s);
    setNearest(n);
    setNotes(runNotes);
    setPhase("autopsy");
  }, []);

  const fail = useCallback((f: Failure) => {
    setFailure(f);
    setPhase("failed");
  }, []);

  const beginSpecimen = useCallback(
    async (day: string) => {
      if (!example) return;
      const strictP = createJob(example.id, day, "strict").then((j) => {
        setProgress((p) => ({ ...p, strict: true }));
        return j;
      });
      const nearestP = createJob(example.id, day, "include-anachronisms").then((j) => {
        setProgress((p) => ({ ...p, nearest: true }));
        return j;
      });
      const [s, n] = await Promise.allSettled([strictP, nearestP]);
      if (s.status === "fulfilled" && n.status === "fulfilled") return succeed(s.value, n.value);
      const reason = s.status === "rejected" ? s.reason : n.status === "rejected" ? n.reason : null;
      fail((reason as Failure) ?? { kind: "network" });
    },
    [example, succeed, fail],
  );

  const beginLive = useCallback(
    async (url: string, day: string) => {
      const token = ++pollToken.current;
      setLiveStage("Starting");
      setLiveRequests(0);
      setElapsed(0);
      try {
        const { id } = await createLive(url, day, rights);
        let misses = 0;
        while (pollToken.current === token) {
          let state;
          try {
            state = await getLive(id);
            misses = 0;
          } catch (e) {
            // One dropped poll is not a failed run. Give up after three in a row.
            if ((e as Failure).kind === "network" && ++misses < 3) {
              await sleep(1500);
              continue;
            }
            throw e;
          }
          if (pollToken.current !== token) return;
          if (state.status === "running") {
            setLiveStage(state.stage);
            setLiveRequests(state.requests);
            await sleep(1500);
            continue;
          }
          if (state.status === "failed") {
            return fail({
              kind: "api",
              status: 422,
              code: state.error.code,
              message: state.error.message,
              verdict: state.error.code === "no_capture" ? "no usable capture" : undefined,
            });
          }
          return succeed(state.strict, state.nearest, state.notes);
        }
      } catch (e) {
        if (pollToken.current === token) fail(e as Failure);
      }
    },
    [rights, succeed, fail],
  );

  const begin = useCallback(
    async (chosenDate?: string) => {
      const day = chosenDate ?? (live ? liveDate : date);
      if (live) setLiveDate(day);
      else setDate(day);
      setPhase("loading");
      setFailure(null);
      setRevealed(false);
      setStrict(null);
      setNearest(null);
      setNotes([]);
      setProgress({ strict: false, nearest: false });
      if (live) await beginLive(liveUrl, day);
      else await beginSpecimen(day);
    },
    [live, liveDate, date, liveUrl, beginLive, beginSpecimen],
  );

  useEffect(() => {
    if (phase === "autopsy") scrollToRef(autopsyRef.current);
    if (phase === "failed") scrollToRef(failRef.current);
  }, [phase]);

  useEffect(() => {
    if (revealed) scrollToRef(revealRef.current);
  }, [revealed]);

  const reset = () => {
    pollToken.current++;
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
  const resultIsLive = strict?.mode === "LIVE";

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
            <div className="seg" role="group" aria-label="What to restore">
              <button
                type="button"
                aria-pressed={!live}
                disabled={busy}
                onClick={() => setSource("specimen")}
              >
                A stored specimen
                <span>Instant. Labelled SEEDED.</span>
              </button>
              <button
                type="button"
                aria-pressed={live}
                disabled={busy}
                onClick={() => setSource("live")}
              >
                A page of your own
                <span>Asks the archive live. About a minute.</span>
              </button>
            </div>

            {!live ? (
              <>
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
              </>
            ) : (
              <>
                <div className="field">
                  <label htmlFor="url">Address of the dead page</label>
                  <input
                    id="url"
                    className="text-input"
                    type="text"
                    inputMode="url"
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="http://www.example.org/about.html"
                    value={liveUrl}
                    onChange={(e) => setLiveUrl(e.target.value)}
                    required
                    disabled={busy}
                  />
                  <p className="fine">
                    One page at a time. Revenant never visits this address. It only asks the archive
                    what it holds.
                  </p>
                  <div className="chips" role="group" aria-label="Try a known page">
                    <button
                      type="button"
                      className="chip"
                      disabled={busy}
                      onClick={() => {
                        setLiveUrl(SAMPLE_LIVE.url);
                        setLiveDate(SAMPLE_LIVE.date);
                      }}
                    >
                      Try the Y2K.gov page, live
                    </button>
                  </div>
                </div>

                <div className="field">
                  <label htmlFor="live-date">Restore it as it was on</label>
                  <input
                    id="live-date"
                    type="date"
                    value={liveDate}
                    min="1996-01-01"
                    onChange={(e) => setLiveDate(e.target.value)}
                    required
                    disabled={busy}
                  />
                </div>

                <label className="check">
                  <input
                    type="checkbox"
                    checked={rights}
                    onChange={(e) => setRights(e.target.checked)}
                    required
                    disabled={busy}
                  />
                  <span>
                    This site is mine, or I have the right to restore it. What the archive holds
                    stays the property of its owners.
                  </span>
                </label>
              </>
            )}

            {busy && live ? (
              <div className="live-progress" aria-live="polite">
                <p className="live-stage">{liveStage}</p>
                <p className="fine">
                  {liveRequests} {liveRequests === 1 ? "question" : "questions"} asked · {elapsed}s ·
                  one at a time, on purpose, so the archive stays polite
                </p>
              </div>
            ) : busy ? (
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
            example={live ? null : example}
            date={live ? liveDate : date}
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
          {notes.map((n) => (
            <p key={n} className="fine">
              {n}
            </p>
          ))}
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
          {resultIsLive ? (
            <li>
              This run asked the Internet Archive live, so it is labelled <strong>LIVE</strong>. The
              dates, hashes and bytes are the archive&rsquo;s, fetched one request at a time.
            </li>
          ) : (
            <li>
              The stored specimen comes from saved copies of public web-archive data, so its runs are
              labelled <strong>SEEDED</strong>. Nothing is simulated: the dates, hashes and bytes are
              the archive&rsquo;s. &ldquo;A page of your own&rdquo; asks the archive live and is
              labelled <strong>LIVE</strong>.
            </li>
          )}
          <li>
            Revenant never calls a missing file a recovered one. A page captured only as an error
            page counts as missing. A file the archive would not answer for is shown as unknown and
            kept out of the score.
          </li>
          <li>
            Pages are restored one at a time, without scripts. Links to other pages are flagged, not
            followed. Restored files are held for an hour.
          </li>
          <li>
            The stored specimen is a US federal government work and in the public domain. Its
            provenance is in the repository.
          </li>
        </ul>
        <p className="fine">
          Open source under the MIT license. <a href={REPO}>{REPO.replace("https://", "")}</a>
        </p>
      </footer>
    </main>
  );
}
