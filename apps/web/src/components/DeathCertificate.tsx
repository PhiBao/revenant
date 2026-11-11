import type { Example, Failure } from "@/lib/api";

interface Props {
  failure: Failure;
  example: Example | null;
  date: string;
  onPickDate: (date: string) => void;
  onRetry: () => void;
  onReset: () => void;
}

/** Every failure stays inside the ritual: a plain statement, then what to do next (W4). */
export function DeathCertificate({ failure, example, date, onPickDate, onRetry, onReset }: Props) {
  if (failure.kind === "network") {
    return (
      <article className="paper paper-dead" role="alert">
        <p className="paper-kicker">The archive room is dark</p>
        <h3 className="paper-title">Revenant could not reach its own server.</h3>
        <p>Check your connection, then try again. Nothing was started.</p>
        <div className="downloads">
          <button type="button" className="btn" onClick={onRetry}>
            Try again
          </button>
        </div>
      </article>
    );
  }

  if (failure.code === "busy") {
    return (
      <article className="paper paper-dead" role="alert">
        <p className="paper-kicker">The slab is full</p>
        <h3 className="paper-title">Other live restores are running.</h3>
        <p>
          Revenant asks the archive one question at a time to stay polite, so only a few live
          restores run together. Try again in a minute, or use the stored specimen.
        </p>
        <div className="downloads">
          <button type="button" className="btn" onClick={onRetry}>
            Try again
          </button>
          <button type="button" className="btn btn-ghost" onClick={onReset}>
            Back to the slab
          </button>
        </div>
      </article>
    );
  }

  if (failure.code === "archive_unavailable") {
    return (
      <article className="paper paper-dead" role="alert">
        <p className="paper-kicker">The archive did not answer</p>
        <h3 className="paper-title">This is not the same as nothing being there.</h3>
        <p>{failure.message}</p>
        <p className="fine">
          Revenant will not call a page lost because the archive was slow or refused a request. Try
          again in a minute.
        </p>
        <div className="downloads">
          <button type="button" className="btn" onClick={onRetry}>
            Try again
          </button>
          <button type="button" className="btn btn-ghost" onClick={onReset}>
            Back to the slab
          </button>
        </div>
      </article>
    );
  }

  if (failure.code === "rate_limited") {
    return (
      <article className="paper paper-dead" role="alert">
        <p className="paper-kicker">One at a time</p>
        <h3 className="paper-title">Too many autopsies in a short time.</h3>
        <p>
          Wait {failure.retryAfter ?? 60} seconds, then try again. This limit keeps the archive
          polite and the service free.
        </p>
        <div className="downloads">
          <button type="button" className="btn" onClick={onRetry}>
            Try again
          </button>
        </div>
      </article>
    );
  }

  if (failure.code === "no_capture") {
    const alternates = (example?.alternate_dates ?? []).filter((a) => a.date !== date);
    return (
      <article className="paper paper-dead" role="alert">
        <p className="paper-kicker">Certificate of death</p>
        <h3 className="paper-title">
          Nothing to bring back for {example ? `${example.site}/${example.page}` : "this page"} on{" "}
          {date}.
        </h3>
        <p>
          The archive has no usable copy of this page from that day or before
          {failure.verdict ? ` (verdict: ${failure.verdict.toLowerCase()})` : ""}. Revenant will
          not invent one.
        </p>
        {failure.nextStep ? <p className="fine">{failure.nextStep}</p> : null}
        {alternates.length > 0 ? (
          <div className="chips" role="group" aria-label="Try another date">
            {alternates.map((a) => (
              <button key={a.date} type="button" className="chip" onClick={() => onPickDate(a.date)}>
                {a.date} · {a.label}
              </button>
            ))}
          </div>
        ) : null}
        <div className="downloads">
          <button type="button" className="btn btn-ghost" onClick={onReset}>
            Back to the slab
          </button>
        </div>
      </article>
    );
  }

  return (
    <article className="paper paper-dead" role="alert">
      <p className="paper-kicker">Could not complete</p>
      <h3 className="paper-title">{failure.message}</h3>
      <div className="downloads">
        <button type="button" className="btn" onClick={onRetry}>
          Try again
        </button>
        <button type="button" className="btn btn-ghost" onClick={onReset}>
          Back to the slab
        </button>
      </div>
    </article>
  );
}
