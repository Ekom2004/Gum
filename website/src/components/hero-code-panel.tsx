"use client";

import type { ReactNode } from "react";

const lines: ReactNode[] = [
  <>
    <span className="text-[#c586c0]">import</span>{" "}
    <span className="text-[#9cdcfe]">gum</span>
  </>,
  "",
  <>
    <span className="text-zinc-500"># Run a long export on Gum-managed compute.</span>
  </>,
  <>
    <span className="text-[#dcdcaa]">@gum.job</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">retries</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">2</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">timeout</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;2h&quot;</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">concurrency</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">1</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    <span className="text-[#c586c0]">def</span>{" "}
    <span className="text-[#dcdcaa]">export_workspace</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">workspace_id</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#4ec9b0]">str</span>
    <span className="text-zinc-300">):</span>
  </>,
  <>
    {"    "}
    <span className="text-[#9cdcfe]">rows</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#dcdcaa]">load_workspace_rows</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">workspace_id</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    {"    "}
    <span className="text-[#9cdcfe]">file_url</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#dcdcaa]">build_csv_export</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">rows</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    {"    "}
    <span className="text-[#dcdcaa]">mark_export_ready</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">workspace_id</span>
    <span className="text-zinc-300">, </span>
    <span className="text-[#9cdcfe]">file_url</span>
    <span className="text-zinc-300">)</span>
  </>,
  "",
  <>
    <span className="text-[#dcdcaa]">export_workspace</span>
    <span className="text-zinc-300">.</span>
    <span className="text-[#dcdcaa]">enqueue</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">workspace_id</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;ws_123&quot;</span>
    <span className="text-zinc-300">)</span>
  </>,
];

export function HeroCodePanel() {
  return (
    <div className="w-full max-w-[620px] text-left">
      <div className="gum-code-surface relative w-full overflow-hidden rounded-sm border border-zinc-800 bg-black text-left">
        <div className="gum-code-body px-4 pt-5 pb-5 font-mono text-[13px] leading-7 text-zinc-200 md:text-sm text-left">
          <div className="space-y-0.5">
            {lines.map((line, index) => (
              <div key={index} className="gum-code-line grid grid-cols-[28px_minmax(0,1fr)] gap-3">
                <span className="gum-code-gutter select-none text-right text-xs">{index + 1}</span>
                <div className="text-left">{line || <span>&nbsp;</span>}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
