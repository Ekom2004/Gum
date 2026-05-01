"use client";

import { useState } from "react";

type CopyableCommandProps = {
  command: string;
  className?: string;
};

export function CopyableCommand({ command, className }: CopyableCommandProps) {
  const [copied, setCopied] = useState(false);

  const copyWithExecCommand = (text: string) => {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "absolute";
    textarea.style.left = "-9999px";
    document.body.appendChild(textarea);
    textarea.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(textarea);
    return ok;
  };

  const setCopiedState = () => {
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };

  const onCopy = async () => {
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(command);
        setCopiedState();
        return;
      }
      if (copyWithExecCommand(command)) {
        setCopiedState();
      }
    } catch {
      if (copyWithExecCommand(command)) {
        setCopiedState();
      }
    }
  };

  return (
    <div className={className}>
      <div className="flex min-w-0 items-center gap-2">
        <span className="shrink-0 font-mono text-zinc-500">$</span>
        <code className="truncate font-mono text-[13px] text-zinc-200">{command}</code>
      </div>
      <button
        type="button"
        onClick={onCopy}
        className="inline-flex h-7 min-w-[64px] shrink-0 items-center justify-center rounded-sm border border-zinc-700 bg-zinc-900 px-2.5 font-mono text-[10px] uppercase tracking-[0.06em] text-zinc-300 transition-colors hover:bg-zinc-800"
        title="Copy"
        aria-label={`Copy command: ${command}`}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}
