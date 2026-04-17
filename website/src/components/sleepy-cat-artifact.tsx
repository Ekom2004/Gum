export function SleepyCatArtifact() {
  return (
    <div className="w-full max-w-[280px] rounded-sm border border-zinc-800 bg-black p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-[11px] uppercase tracking-[0.18em] text-zinc-500">Artifact</span>
        <span className="text-[11px] uppercase tracking-[0.18em] text-zinc-600">Sleep mode</span>
      </div>

      <svg
        aria-hidden="true"
        viewBox="0 0 240 160"
        className="w-full text-[var(--gum-paper)]"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M26 118h188" className="text-zinc-700" />
        <path d="M42 118V94h156v24" className="text-zinc-600" />
        <path d="M52 94c10-14 26-22 44-22h34c20 0 37 8 49 22" className="text-zinc-700" />
        <path d="M73 100h92c16 0 28 8 34 18H39c6-10 18-18 34-18Z" className="text-[var(--gum-paper)]" />
        <path d="M97 73l8-12 11 10" />
        <path d="M144 73l-8-12-11 10" />
        <path d="M92 79c0-11 9-20 20-20h17c16 0 29 13 29 29 0 8-3 14-8 20" />
        <path d="M85 92c4 12 16 20 30 20h19" />
        <path d="M100 83c4 3 8 4 12 4" />
        <path d="M148 82c-4 3-8 4-12 4" />
        <path d="M117 95c4 2 8 3 13 3" />
        <path d="M156 96c6 2 11 5 15 10" className="text-zinc-500" />
        <path d="M173 52c7-5 10-12 10-20" className="text-zinc-600" />
        <path d="M186 58c7-5 10-12 10-20" className="text-zinc-600" />
      </svg>
    </div>
  );
}
