"use client";

import { useEffect, useState, type ReactNode } from "react";

type TerminalLine = {
  text: string;
  tone?: "muted" | "ok" | "hint";
};

type HeroViewMode = "code" | "terminal";

const codeLines: ReactNode[] = [
  <>
    <span className="text-[#c586c0]">from</span>{" "}
    <span className="text-[#9cdcfe]">usegum</span>{" "}
    <span className="text-[#c586c0]">import</span>{" "}
    <span className="text-[#9cdcfe]">gum</span>
  </>,
  <>
    <span className="text-[#c586c0]">import</span>{" "}
    <span className="text-[#9cdcfe]">resend</span>
  </>,
  <>
    <span className="text-zinc-500">
      {
        "# Weekly digest runs every Monday at 09:00 New York time; retries recover transient failures and concurrency=1 prevents overlap."
      }
    </span>
  </>,
  <>
    <span className="text-[#dcdcaa]">@gum.job</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">cron</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;0 9 * * 1&quot;</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">timezone</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;America/New_York&quot;</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">concurrency</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">1</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">retries</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">5</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    <span className="text-[#c586c0]">def</span>{" "}
    <span className="text-[#dcdcaa]">send_weekly_digest</span>
    <span className="text-zinc-300">():</span>
  </>,
  <>
    {"    "}
    <span className="text-[#9cdcfe]">stats</span>
    <span className="text-zinc-500"> = </span>
    <span className="text-[#9cdcfe]">db</span>
    <span className="text-zinc-300">.</span>
    <span className="text-[#dcdcaa]">get_weekly_metrics</span>
    <span className="text-zinc-300">()</span>
  </>,
  <>
    {"    "}
    <span className="text-[#9cdcfe]">resend</span>
    <span className="text-zinc-300">.</span>
    <span className="text-[#dcdcaa]">emails</span>
    <span className="text-zinc-300">.</span>
    <span className="text-[#dcdcaa]">send</span>
    <span className="text-zinc-300">(</span>
  </>,
  <>
    {"        "}
    <span className="text-[#9cdcfe]">from_</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;ops@acme.com&quot;</span>
    <span className="text-zinc-300">, </span>
    <span className="text-[#9cdcfe]">to</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;team@acme.com&quot;</span>
    <span className="text-zinc-300">,</span>
  </>,
  <>
    {"        "}
    <span className="text-[#9cdcfe]">subject</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;Weekly Performance Digest&quot;</span>
    <span className="text-zinc-300">, </span>
    <span className="text-[#9cdcfe]">html</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#dcdcaa]">render_template</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">stats</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    {"    "}
    <span className="text-zinc-300">)</span>
  </>,
];

const deployLines: TerminalLine[] = [
  { text: "$ gum deploy" },
  { text: "Packaging project..." },
  { text: "Project: proj_live", tone: "muted" },
  { text: "API: https://api.gum.cloud", tone: "muted" },
  { text: "Found 1 function", tone: "muted" },
  {
    text: '- send_weekly_digest [cron="0 9 * * 1" timezone=America/New_York concurrency=1 retries=5]',
  },
  { text: "Deploy: dep_7Qp9a2" },
  { text: "Status: active", tone: "ok" },
  { text: "Next run: Mon 09:00 America/New_York", tone: "hint" },
];

const idleLines: TerminalLine[] = [
  { text: "Click >_ to run a live deploy demo", tone: "hint" },
  { text: "$ gum deploy", tone: "muted" },
];

export function HeroCodePanel() {
  const [mode, setMode] = useState<HeroViewMode>("code");
  const [running, setRunning] = useState(false);
  const [visibleCount, setVisibleCount] = useState(0);

  useEffect(() => {
    if (mode !== "terminal" || !running) {
      return;
    }
    if (visibleCount >= deployLines.length) {
      setRunning(false);
      return;
    }
    const timer = window.setTimeout(() => {
      setVisibleCount((current) => current + 1);
    }, 140);
    return () => window.clearTimeout(timer);
  }, [running, visibleCount, mode]);

  const startTerminalDemo = () => {
    setMode("terminal");
    setVisibleCount(0);
    setRunning(true);
  };

  const showCode = () => {
    setMode("code");
    setRunning(false);
    setVisibleCount(0);
  };

  const visibleTerminalLines = visibleCount > 0 ? deployLines.slice(0, visibleCount) : idleLines;

  const lineToneClass = (tone?: TerminalLine["tone"]) => {
    if (tone === "ok") {
      return "text-emerald-300";
    }
    if (tone === "hint") {
      return "text-zinc-400";
    }
    if (tone === "muted") {
      return "text-zinc-500";
    }
    return "text-zinc-200";
  };

  const terminalGlyph = (line: TerminalLine) => {
    if (line.text.startsWith("$")) {
      return "$";
    }
    if (line.tone === "ok") {
      return "●";
    }
    if (line.tone === "muted") {
      return "·";
    }
    return "›";
  };

  const terminalGlyphClass = (tone?: TerminalLine["tone"], text?: string) => {
    if (text?.startsWith("$")) {
      return "text-zinc-400";
    }
    if (tone === "ok") {
      return "text-emerald-300";
    }
    if (tone === "muted") {
      return "text-zinc-600";
    }
    return "text-zinc-500";
  };

  return (
    <div className="w-full max-w-[680px] text-left">
      <div className="relative overflow-hidden rounded-sm border border-zinc-800 bg-zinc-950">
        <div className="flex h-11 items-center border-b border-zinc-800 bg-zinc-900/90 px-4">
          <span className="font-mono text-xs text-zinc-500">{mode === "code" ? "worker.py" : "terminal"}</span>
        </div>

        <div className="h-[29rem] overflow-hidden bg-black px-5 py-6 font-mono text-[13px] leading-8">
          <div className="space-y-0.5">
            {mode === "code"
              ? codeLines.map((line, index) => (
                  <div key={`code-${index}`} className="grid grid-cols-[28px_minmax(0,1fr)] gap-3">
                    <span className="gum-code-gutter select-none text-right text-xs">{index + 1}</span>
                    <div
                      className={`text-left ${index === 3 ? "whitespace-nowrap tracking-[-0.01em]" : ""}`}
                    >
                      {line || <span>&nbsp;</span>}
                    </div>
                  </div>
                ))
              : visibleTerminalLines.map((line, index) => (
                  <div key={`terminal-${index}`} className="grid grid-cols-[14px_minmax(0,1fr)] gap-3">
                    <span
                      className={`select-none text-right text-xs ${terminalGlyphClass(line.tone, line.text)}`}
                    >
                      {terminalGlyph(line)}
                    </span>
                    <div className={`text-left transition-opacity ${lineToneClass(line.tone)}`}>
                      {line.text || <span>&nbsp;</span>}
                    </div>
                  </div>
                ))}
            {mode === "terminal" && running ? <div className="mt-2 text-left text-zinc-500">...</div> : null}
          </div>
        </div>

        <button
          type="button"
          onClick={mode === "code" ? startTerminalDemo : showCode}
          className="absolute right-3 bottom-3 inline-flex h-9 items-center justify-center rounded-sm border border-zinc-700 bg-zinc-950/95 px-3 font-mono text-[11px] text-zinc-200 transition-colors hover:bg-zinc-900"
        >
          {mode === "code" ? ">_ Terminal" : "< back to code"}
        </button>
      </div>
    </div>
  );
}
