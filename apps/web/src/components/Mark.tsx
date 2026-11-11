/** The mark: a tombstone cut by one line that runs flat, then pulses. Same drawing as favicon.svg. */
export function Mark({ size = 28 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      role="img"
      aria-label="Revenant"
      className="mark"
    >
      <path d="M6 30V14a10 10 0 0 1 20 0v16z" fill="currentColor" />
      <path
        d="M7.1 20h5.9l2.2-6.5L18.4 25l2.3-5h4.2"
        fill="none"
        stroke="var(--ink)"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
