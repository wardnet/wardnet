/** Description slot used in progress toasts — a thin progress bar plus the
 *  numeric percentage. Fed by the polled job status. */
export function JobProgressDescription({ percentage }: { percentage: number }) {
  const pct = Math.max(0, Math.min(100, percentage));
  return (
    <div className="job-progress">
      <div className="job-progress__track">
        <div className="job-progress__bar" style={{ width: `${pct}%` }} />
      </div>
      <span className="job-progress__pct">{pct}%</span>
    </div>
  );
}
