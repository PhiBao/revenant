"use client";

import type { Timeline } from "@/lib/api";

const NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const LONG = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** Last day of a month as `YYYY-MM-DD`. `month` is `YYYYMM`. */
export function endOfMonth(month: string): string {
  const y = Number(month.slice(0, 4));
  const m = Number(month.slice(4, 6));
  const last = new Date(Date.UTC(y, m, 0)).getUTCDate();
  return `${month.slice(0, 4)}-${month.slice(4, 6)}-${String(last).padStart(2, "0")}`;
}

export function MonthStrip({
  timeline,
  chosen,
  onPick,
}: {
  timeline: Timeline;
  chosen: string;
  onPick: (date: string) => void;
}) {
  if (timeline.months.length === 0) {
    return (
      <p className="fine">
        The archive holds no capture of this address. That is a finding about the archive, not a
        failure.
      </p>
    );
  }
  const have = new Set(timeline.months.map((m) => m.month));
  const firstYear = Number(timeline.months[0]!.month.slice(0, 4));
  const lastYear = Number(timeline.months[timeline.months.length - 1]!.month.slice(0, 4));
  const years: number[] = [];
  for (let y = firstYear; y <= lastYear; y++) years.push(y);
  const chosenMonth = chosen ? chosen.slice(0, 7).replace("-", "") : "";

  return (
    <div className="strip" role="group" aria-label="Months the archive holds a capture">
      <p className="fine">
        {timeline.months.length === 1 ? "1 month has" : `${timeline.months.length} months have`} at least one capture. The archive holds at least one
        capture in each filled month. It does not say the page changed.
        {timeline.truncated ? " Only the first 600 months are shown." : ""}
      </p>
      {years.map((y) => (
        <div className="strip-year" key={y}>
          <span className="strip-label">{y}</span>
          <div className="strip-months">
            {NAMES.map((n, i) => {
              const key = `${y}${String(i + 1).padStart(2, "0")}`;
              if (!have.has(key)) {
                return (
                  <span className="strip-cell empty" key={key} aria-hidden="true">
                    {n}
                  </span>
                );
              }
              return (
                <button
                  type="button"
                  key={key}
                  className="strip-cell"
                  aria-pressed={chosenMonth === key}
                  aria-label={`${LONG[i]} ${y}, use the end of this month`}
                  onClick={() => onPick(endOfMonth(key))}
                >
                  {n}
                </button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
