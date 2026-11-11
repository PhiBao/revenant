export type Verdict =
  | "PROVEN"
  | "ATTESTED"
  | "LIKELY"
  | "UNCERTAIN"
  | "ANACHRONISM"
  | "MISSING"
  | "UNKNOWN";

export interface FileRecord {
  path: string;
  verdict: Verdict;
  /** What capture dates alone would have said. */
  verdict_from_dates_only: Verdict;
  /** Why the file got its verdict, in plain words. */
  basis: string;
  included: boolean;
  capture_time: string | null;
  /** The origin server's own Last-Modified for the chosen capture. */
  origin_modified: string | null;
  delta_days: number | null;
  archive_digest: string | null;
  sha256: string | null;
  bytes: number | null;
  note: string | null;
}

export interface Vitals {
  completeness_percent: number | null;
  fidelity_percent: number | null;
  unknown_files: number;
}

export interface Certificate {
  schema: string;
  site: string;
  page: string;
  target_time: string;
  mode: "LIVE" | "SEEDED";
  archive: string;
  policy: string;
  vitals: Vitals;
  /** The same files scored from capture dates alone. */
  vitals_from_dates_only: Vitals;
  files: FileRecord[];
  links_not_restored: string[];
  scripts_removed: number;
  archive_urls_unwrapped: number;
  archive_host_text_mentions: number;
  notes: string[];
}

export interface Job {
  id: string;
  example: string;
  date: string;
  policy: "strict" | "with-anachronisms" | "nearest";
  mode: "LIVE" | "SEEDED";
  expires_in_secs: number;
  zip_sha256: string;
  zip_bytes: number;
  certificate: Certificate;
  urls: { self: string; zip: string; preview: string };
}

export interface Example {
  id: string;
  title: string;
  site: string;
  page: string;
  default_date: string;
  alternate_dates: { date: string; label: string }[];
  blurb: string;
  rights: string;
}

export type Failure =
  | {
      kind: "api";
      status: number;
      code: string;
      message: string;
      verdict?: string;
      nextStep?: string;
      retryAfter?: number;
    }
  | { kind: "network" };

export type PolicyName = "strict" | "include-anachronisms" | "nearest";

async function parseFailure(res: Response): Promise<Failure> {
  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    body = null;
  }
  const err = (body as { error?: Record<string, unknown> } | null)?.error ?? {};
  const retry = Number(res.headers.get("retry-after"));
  return {
    kind: "api",
    status: res.status,
    code: typeof err.code === "string" ? err.code : "error",
    message: typeof err.message === "string" ? err.message : "The request failed.",
    verdict: typeof err.verdict === "string" ? err.verdict : undefined,
    nextStep: typeof err.next_step === "string" ? err.next_step : undefined,
    retryAfter: Number.isFinite(retry) && retry > 0 ? retry : undefined,
  };
}

export async function listExamples(): Promise<Example[]> {
  let res: Response;
  try {
    res = await fetch("/api/examples");
  } catch {
    throw { kind: "network" } satisfies Failure;
  }
  if (!res.ok) throw await parseFailure(res);
  const data = (await res.json()) as { examples: Example[] };
  return data.examples;
}

export async function createJob(
  example: string,
  date: string,
  policy: PolicyName,
): Promise<Job> {
  let res: Response;
  try {
    res = await fetch("/api/resurrections", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ example, date, policy }),
    });
  } catch {
    throw { kind: "network" } satisfies Failure;
  }
  if (!res.ok) throw await parseFailure(res);
  return (await res.json()) as Job;
}

export type LiveState =
  | { status: "running"; stage: string; requests: number; notes: string[] }
  | { status: "failed"; error: { code: string; message: string }; notes: string[] }
  | { status: "done"; strict: Job; nearest: Job; notes: string[] };

export async function createLive(url: string, date: string, rights: boolean): Promise<{ id: string }> {
  let res: Response;
  try {
    res = await fetch("/api/live", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url, date, rights }),
    });
  } catch {
    throw { kind: "network" } satisfies Failure;
  }
  if (!res.ok) throw await parseFailure(res);
  return (await res.json()) as { id: string };
}

export async function getLive(id: string): Promise<LiveState> {
  let res: Response;
  try {
    res = await fetch(`/api/live/${id}`);
  } catch {
    throw { kind: "network" } satisfies Failure;
  }
  if (!res.ok) throw await parseFailure(res);
  return (await res.json()) as LiveState;
}

/** `20000301091452` becomes `2000-03-01 09:14 UTC`. */
export function formatCapture(cdx: string | null): string {
  if (!cdx || cdx.length !== 14) return "—";
  return `${cdx.slice(0, 4)}-${cdx.slice(4, 6)}-${cdx.slice(6, 8)} ${cdx.slice(8, 10)}:${cdx.slice(10, 12)} UTC`;
}

export function formatDelta(days: number | null): string {
  if (days === null) return "—";
  if (days === 0) return "same day";
  const abs = Math.abs(days);
  const unit = abs === 1 ? "day" : "days";
  return days > 0 ? `+${abs} ${unit}` : `−${abs} ${unit}`;
}

export function formatPercent(p: number | null): string {
  return p === null ? "—" : p.toFixed(1);
}

/** `20000301091452` becomes `2000-03-01`. */
export function formatDay(cdx: string | null): string {
  if (!cdx || cdx.length < 8) return "—";
  return `${cdx.slice(0, 4)}-${cdx.slice(4, 6)}-${cdx.slice(6, 8)}`;
}

/** True when two restores contain the same files with the same bytes (W6.4). */
export function sameRestore(a: Certificate, b: Certificate): boolean {
  const key = (c: Certificate) =>
    c.files
      .filter((f) => f.included)
      .map((f) => `${f.path}:${f.sha256}`)
      .sort()
      .join("|");
  return key(a) === key(b);
}
