"use client";

import { useId, useState } from "react";

interface Props {
  /** The "before" restore, drawn on the left of the divider. */
  nearestUrl: string;
  /** The restore that keeps only what can be shown as it was on the day. */
  dayUrl: string;
  dayLabel: string;
  /** What the left side is, for example "What archive playback gives". */
  leftLabel?: string;
  /** Short name of the left side for the toggle button. */
  leftShort?: string;
}

/**
 * Two restorations of the same page over one viewport. The nearest-capture restore sits on the
 * left of the divider, the as-it-was restore on the right. Both previews are sandboxed (W2.3).
 */
export function Curtain({
  nearestUrl,
  dayUrl,
  dayLabel,
  leftLabel = "What archive playback gives",
  leftShort = "Archive playback",
}: Props) {
  const [pos, setPos] = useState(50);
  const id = useId();

  return (
    <figure className="curtain">
      <div className="curtain-labels" aria-hidden="true">
        <span>{leftLabel}</span>
        <span>{dayLabel}</span>
      </div>

      <div className="curtain-view">
        <iframe
          className="curtain-frame curtain-frame-day"
          src={dayUrl}
          title={`Restoration as it was on ${dayLabel}`}
          sandbox=""
        />
        <iframe
          className="curtain-frame curtain-frame-nearest"
          src={nearestUrl}
          title={leftLabel}
          sandbox=""
          style={{ clipPath: `inset(0 ${100 - pos}% 0 0)` }}
        />
        <div className="curtain-line" style={{ left: `${pos}%` }} aria-hidden="true" />
      </div>

      <div className="curtain-controls">
        <label htmlFor={id} className="curtain-slider-label">
          Drag to compare
        </label>
        <input
          id={id}
          className="curtain-slider"
          type="range"
          min={0}
          max={100}
          value={pos}
          onChange={(e) => setPos(Number(e.target.value))}
          aria-valuetext={`${pos} percent of the left side shown`}
        />
        <div className="curtain-toggle" role="group" aria-label="Choose a restoration">
          <button type="button" aria-pressed={pos === 100} onClick={() => setPos(100)}>
            {leftShort}
          </button>
          <button type="button" aria-pressed={pos === 0} onClick={() => setPos(0)}>
            As it was
          </button>
        </div>
      </div>
    </figure>
  );
}
