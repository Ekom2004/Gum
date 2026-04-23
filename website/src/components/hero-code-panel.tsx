"use client";

import { useEffect, useState, type ReactNode } from "react";

type TerminalLine = {
  text: string;
  tone?: "muted" | "ok" | "hint";
};

type HeroViewMode = "code" | "terminal";

const codeLines: ReactNode[] = [
  <>
    <span className="text-[#c586c0]">import</span>{" "}
    <span className="text-[#9cdcfe]">gum</span>
  </>,
  "",
  <>
    <span className="text-zinc-500">
      # Process Stripe events once, even when Stripe retries delivery.
    </span>
  </>,
  "",
  <>
    <span className="text-[#dcdcaa]">@gum.job</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">retries</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">5</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">timeout</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;5m&quot;</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">concurrency</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#b5cea8]">5</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">key</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;event_id&quot;</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    <span className="text-[#c586c0]">def</span>{" "}
    <span className="text-[#dcdcaa]">process_stripe_event</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">event_id</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#4ec9b0]">str</span>
    <span className="text-zinc-300">, </span>
    <span className="text-[#9cdcfe]">event</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#4ec9b0]">dict</span>
    <span className="text-zinc-300">):</span>
  </>,
  <>
    {"    "}
    <span className="text-[#c586c0]">if</span>{" "}
    <span className="text-[#9cdcfe]">event</span>
    <span className="text-zinc-300">[</span>
    <span className="text-[#ce9178]">&quot;type&quot;</span>
    <span className="text-zinc-300">] == </span>
    <span className="text-[#ce9178]">&quot;payment_intent.succeeded&quot;</span>
    <span className="text-zinc-300">:</span>
  </>,
  <>
    {"        "}
    <span className="text-[#9cdcfe]">customer_id</span>
    <span className="text-zinc-500"> = </span>
    <span className="text-[#9cdcfe]">event</span>
    <span className="text-zinc-300">(</span>
    <span className="text-zinc-300">[</span>
    <span className="text-[#ce9178]">&quot;data&quot;</span>
    <span className="text-zinc-300">][</span>
    <span className="text-[#ce9178]">&quot;object&quot;</span>
    <span className="text-zinc-300">][</span>
    <span className="text-[#ce9178]">&quot;customer&quot;</span>
    <span className="text-zinc-300">]</span>
  </>,
  <>
    {"        "}
    <span className="text-[#dcdcaa]">grant_access</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">customer_id</span>
    <span className="text-zinc-300">)</span>
  </>,
  <>
    {"    "}
    <span className="text-[#c586c0]">return</span>{" "}
    <span className="text-zinc-300">{"{"}</span>
    <span className="text-[#ce9178]">&quot;event_id&quot;</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#9cdcfe]">event_id</span>
    <span className="text-zinc-300">, </span>
    <span className="text-[#ce9178]">&quot;status&quot;</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#ce9178]">&quot;processed&quot;</span>
    <span className="text-zinc-300">{"}"}</span>
  </>,
  "",
  <>
    <span className="text-[#dcdcaa]">process_stripe_event</span>
    <span className="text-zinc-300">.</span>
    <span className="text-[#dcdcaa]">enqueue</span>
    <span className="text-zinc-300">(</span>
    <span className="text-[#9cdcfe]">event_id</span>
    <span className="text-zinc-500">=</span>
    <span className="text-[#ce9178]">&quot;evt_123&quot;</span>
    <span className="text-zinc-300">,</span>
    <span className="text-[#9cdcfe]">event</span>
    <span className="text-zinc-500">=</span>
    <span className="text-zinc-300">{"{"}</span>
    <span className="text-[#ce9178]">&quot;type&quot;</span>
    <span className="text-zinc-300">: </span>
    <span className="text-[#ce9178]">&quot;payment_intent.succeeded&quot;</span>
    <span className="text-zinc-300">{"}"}</span>
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
    text: "- process_stripe_event [retries=5 timeout=300s concurrency=5 key=event_id]",
  },
  { text: "Deploy: dep_7Qp9a2" },
  { text: "Status: active", tone: "ok" },
  { text: "Next: gum run process_stripe_event --event-id evt_123", tone: "hint" },
];

const idleLines: TerminalLine[] = [
  { text: "Click >_ to run a live deploy demo", tone: "hint" },
  { text: "$ gum deploy", tone: "muted" },
];

const FIXED_ROWS = 12;

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
  }, [running, visibleCount]);

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
  const paddedCodeLines = [...codeLines];
  while (paddedCodeLines.length < FIXED_ROWS) {
    paddedCodeLines.push("");
  }
  const paddedTerminalLines = [...visibleTerminalLines];
  while (paddedTerminalLines.length < FIXED_ROWS) {
    paddedTerminalLines.push({ text: "" });
  }

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

  return (
    <div className="w-full max-w-[620px] text-left">
      <div className="gum-code-surface relative w-full overflow-hidden rounded-sm border border-zinc-800 bg-black text-left">
        <div className="flex h-10 items-center justify-between border-b border-zinc-800 bg-zinc-900 px-3">
          <div className="flex items-center gap-2">
            <span className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]"></span>
            <span className="h-2.5 w-2.5 rounded-full bg-[#febc2e]"></span>
            <span className="h-2.5 w-2.5 rounded-full bg-[#28c840]"></span>
            <span className="ml-2 text-xs font-mono text-zinc-500">
              {mode === "code" ? "worker.py" : "terminal"}
            </span>
          </div>
          <div className="inline-flex items-center gap-2">
            {mode === "terminal" ? (
              <button
                type="button"
                onClick={showCode}
                className="inline-flex items-center rounded border border-zinc-700 px-2 py-0.5 font-mono text-[11px] text-zinc-300 transition-colors hover:border-zinc-500 hover:bg-zinc-800"
              >
                py
              </button>
            ) : null}
            <button
              type="button"
              onClick={startTerminalDemo}
              className="inline-flex items-center rounded border border-zinc-700 px-2 py-0.5 font-mono text-[11px] text-zinc-300 transition-colors hover:border-zinc-500 hover:bg-zinc-800"
            >
              &gt;_
            </button>
          </div>
        </div>
        <div
          className={`gum-code-body h-[380px] px-4 pt-5 pb-5 font-mono text-[13px] leading-7 text-zinc-200 md:text-sm text-left ${
            mode === "code"
              ? "overflow-x-auto overflow-y-hidden gum-scrollbar-none"
              : "overflow-hidden"
          }`}
        >
          <div className="space-y-0.5">
            {mode === "code"
              ? paddedCodeLines.map((line, index) => (
                  <div
                    key={`code-${index}`}
                    className="gum-code-line grid grid-cols-[28px_minmax(0,1fr)] gap-3"
                  >
                    <span className="gum-code-gutter select-none text-right text-xs">{index + 1}</span>
                    <div className="min-w-max text-left whitespace-nowrap">
                      {line || <span>&nbsp;</span>}
                    </div>
                  </div>
                ))
              : paddedTerminalLines.map((line, index) => (
                  <div
                    key={`terminal-${index}`}
                    className="gum-code-line grid grid-cols-[28px_minmax(0,1fr)] gap-3"
                  >
                    <span className="gum-code-gutter select-none text-right text-xs">{index + 1}</span>
                    <div className={`text-left transition-opacity ${lineToneClass(line.tone)}`}>
                      {line.text || <span>&nbsp;</span>}
                    </div>
                  </div>
                ))}
            {mode === "terminal" && running ? (
              <div className="mt-2 text-left text-zinc-500">...</div>
            ) : null}
          </div>
          {mode === "code" ? (
            <div className="mt-4 text-[11px] uppercase tracking-[0.14em] text-zinc-500">
              Click &gt;_ to run deploy demo
            </div>
          ) : (
            <div className="mt-4 text-[11px] uppercase tracking-[0.14em] text-zinc-500">
              Click py to return to code
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
