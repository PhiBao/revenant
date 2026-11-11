/**
 * A heart trace. `alive` false draws a flat line in the color of the dead; true draws the pulse in
 * the color of life. The pulse is drawn once; with reduced motion it is simply present (W5.2).
 */
export function Ekg({ alive, label }: { alive: boolean; label: string }) {
  return (
    <svg
      className={`ekg ${alive ? "ekg-alive" : "ekg-flat"}`}
      viewBox="0 0 600 60"
      preserveAspectRatio="none"
      role="img"
      aria-label={label}
    >
      <line x1="0" y1="30" x2="600" y2="30" className="ekg-base" />
      {alive ? (
        <path
          className="ekg-pulse"
          pathLength={1}
          d="M0 30H210l10-4 8 4h16l8-22 12 44 10-34 8 12h18l10-5 8 5H600"
        />
      ) : null}
    </svg>
  );
}
