"use client";

import { useState, type ReactNode } from "react";

type CodeExample = {
  id: "scheduled" | "enqueued";
  label: string;
  lines: ReactNode[];
};

const examples: CodeExample[] = [
  {
    id: "scheduled",
    label: "Scheduled",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">resend</span>
      </>,
      "",
      <>
        <span className="text-zinc-500"># Run a follow-up email every 20 days.</span>
      </>,
      <>
        <span className="text-[#dcdcaa]">@gum.job</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">every</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;20d&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">retries</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">5</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">rate_limit</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;20/m&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">timeout</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;5m&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        <span className="text-[#c586c0]">def</span>{" "}
        <span className="text-[#dcdcaa]">send_followup</span>
        <span className="text-zinc-300">():</span>
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
        <span className="text-[#ce9178]">&quot;Acme &lt;hello@acme.com&gt;&quot;</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">to</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;alex@example.com&quot;</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">subject</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;Checking in&quot;</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">html</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;&lt;p&gt;Hey Alex, just checking in.&lt;/p&gt;&quot;</span>
      </>,
      <>
        {"    "}
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "enqueued",
    label: "Enqueued",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">salesforce</span>
      </>,
      "",
      <>
        <span className="text-zinc-500"># Sync each customer without overloading Salesforce.</span>
      </>,
      <>
        <span className="text-[#dcdcaa]">@gum.job</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">retries</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">8</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">timeout</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;15m&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">rate_limit</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;20/m&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">concurrency</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">5</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        <span className="text-[#c586c0]">def</span>{" "}
        <span className="text-[#dcdcaa]">sync_customer</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">customer_id</span>
        <span className="text-zinc-300">: </span>
        <span className="text-[#4ec9b0]">str</span>
        <span className="text-zinc-300">):</span>
      </>,
      <>
        {"    "}
        <span className="text-[#9cdcfe]">salesforce</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">upsert_customer</span>
        <span className="text-zinc-300">(</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">customer_id</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"    "}
        <span className="text-zinc-300">)</span>
      </>,
      "",
      <>
        <span className="text-[#dcdcaa]">sync_customer</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">enqueue</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">customer_id</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;cus_123&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
];

export function HeroCodePanel() {
  const [activeId, setActiveId] = useState<CodeExample["id"]>("scheduled");
  const active = examples.find((example) => example.id === activeId) ?? examples[0];

  return (
    <div className="w-full max-w-[620px] text-left">
      <div className="gum-code-surface relative w-full overflow-hidden rounded-sm border border-zinc-800 bg-black text-left">
        <div className="flex px-4 pt-3 pb-1">
          <div className="inline-flex rounded-sm border border-zinc-800 bg-zinc-950 p-1">
            {examples.map((example) => {
              const isActive = example.id === active.id;
              return (
                <button
                  key={example.id}
                  type="button"
                  onClick={() => setActiveId(example.id)}
                  className={`rounded-sm px-3 py-1.5 text-xs font-medium transition-colors ${
                    isActive
                      ? "bg-zinc-900 text-zinc-100"
                      : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-100"
                  }`}
                >
                  {example.label}
                </button>
              );
            })}
          </div>
        </div>
        <div className="bg-black px-4 pt-2 pb-5 font-mono text-[13px] leading-7 text-[#e4e4e7] md:text-sm text-left">
          <div className="space-y-0.5">
            {active.lines.map((line, index) => (
              <div key={`${active.id}-${index}`} className="gum-code-line grid grid-cols-[28px_minmax(0,1fr)] gap-3">
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
