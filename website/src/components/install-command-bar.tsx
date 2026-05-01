"use client";

import { useState } from "react";

const INSTALL_COMMAND = "pip install usegum && gum deploy";

export function InstallCommandBar() {
  const [copied, setCopied] = useState(false);

  const copyCommand = async () => {
    try {
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="mt-6 w-full max-w-[540px] rounded-sm border border-zinc-800 bg-zinc-950">
      <div className="flex items-center justify-between border-b border-zinc-800 px-3 py-2">
        <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
          Quick start
        </span>
        <button
          type="button"
          onClick={copyCommand}
          className="inline-flex h-7 items-center justify-center rounded-sm border border-zinc-700 px-3 font-mono text-[11px] uppercase tracking-[0.08em] text-zinc-200 transition-colors hover:bg-zinc-900"
          aria-live="polite"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <div className="flex items-center gap-2 overflow-x-auto px-3 py-3 font-mono text-[13px] text-zinc-200 gum-scrollbar-none">
        <span className="shrink-0 text-zinc-500">$</span>
        <code className="whitespace-nowrap">{INSTALL_COMMAND}</code>
      </div>
    </div>
  );
}
