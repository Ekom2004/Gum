"use client";

import type { ReactNode } from "react";

type UseCase = {
  id: "exports" | "uploads" | "scheduled" | "rate_limited";
  tabLabel: string;
  label: string;
  description: string;
  lines: ReactNode[];
};

const useCases: UseCase[] = [
  {
    id: "exports",
    tabLabel: "Heavy exports",
    label: "Run heavy exports without blocking the app",
    description: "Move CSV and report generation off the request path and let Gum own the runtime.",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      "",
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
    ],
  },
  {
    id: "uploads",
    tabLabel: "File processing",
    label: "Process uploads in the background",
    description: "Run file-heavy work off the request path with bounded concurrency and long timeouts.",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      "",
      <>
        <span className="text-[#dcdcaa]">@gum.job</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">retries</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">3</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">timeout</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;30m&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">concurrency</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">2</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        <span className="text-[#c586c0]">def</span>{" "}
        <span className="text-[#dcdcaa]">process_upload</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">upload_id</span>
        <span className="text-zinc-300">: </span>
        <span className="text-[#4ec9b0]">str</span>
        <span className="text-zinc-300">):</span>
      </>,
      <>
        {"    "}
        <span className="text-[#dcdcaa]">extract_text</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">upload_id</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        {"    "}
        <span className="text-[#dcdcaa]">generate_preview</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">upload_id</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        {"    "}
        <span className="text-[#dcdcaa]">store_search_chunks</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">upload_id</span>
        <span className="text-zinc-300">)</span>
      </>,
      "",
      <>
        <span className="text-[#dcdcaa]">process_upload</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">enqueue</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">upload_id</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;upl_123&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "scheduled",
    tabLabel: "Scheduled jobs",
    label: "Run scheduled work without cron glue",
    description: "Put lifecycle jobs on a schedule directly in code and let Gum fire them on time.",
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
        <span className="text-[#ce9178]">&quot;&lt;p&gt;Just checking in on your trial.&lt;/p&gt;&quot;</span>
      </>,
      <>
        {"    "}
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "rate_limited",
    tabLabel: "Rate-limited APIs",
    label: "Call rate-limited APIs safely",
    description: "Keep third-party APIs under control with rate limits and bounded concurrency.",
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
        {"        "}
        <span className="text-[#9cdcfe]">sync_invoices</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#4ec9b0]">True</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">sync_subscriptions</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#4ec9b0]">True</span>
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

export function UseCasesPanel() {
  return (
    <div className="gum-use-cases grid grid-cols-1 items-stretch gap-12 lg:grid-cols-12 lg:gap-8">
      {useCases.map((item, index) => (
        <input
          key={item.id}
          id={`gum-use-case-${item.id}`}
          name="gum-use-case"
          type="radio"
          className="gum-use-case-input"
          defaultChecked={index === 0}
        />
      ))}

      <div className="lg:col-span-5">
        <div className="gum-use-case-menu grid h-full grid-cols-1 gap-2 lg:grid-rows-4">
          {useCases.map((item) => (
            <label
              key={item.id}
              htmlFor={`gum-use-case-${item.id}`}
              data-use-case-tab={item.id}
              className="flex cursor-pointer flex-col justify-center rounded-r-lg border-l-2 border-transparent px-4 py-3 text-left text-zinc-500 transition-colors hover:text-zinc-300"
            >
              <span className="block text-sm font-medium tracking-[-0.01em]">{item.tabLabel}</span>
              <span className="mt-1 block max-w-md text-sm leading-relaxed text-zinc-500">
                {item.description}
              </span>
            </label>
          ))}
        </div>
      </div>

      <div className="gum-use-case-panels lg:col-span-7 lg:sticky lg:top-24">
        <div className="gum-code-surface relative flex h-full w-full overflow-hidden rounded-xl border border-zinc-800 bg-black text-left">
          <div className="flex min-h-full w-full flex-col">
            <div className="flex h-10 shrink-0 items-center gap-3 border-b border-zinc-800 bg-zinc-900 px-4">
              <div className="flex items-center gap-1.5" aria-hidden="true">
                <span className="h-2.5 w-2.5 rounded-full bg-[#ff5f57]" />
                <span className="h-2.5 w-2.5 rounded-full bg-[#ffbd2e]" />
                <span className="h-2.5 w-2.5 rounded-full bg-[#28c840]" />
              </div>
              <span className="font-mono text-xs text-zinc-500">worker.py</span>
            </div>
            <div className="gum-use-case-code-stack grid flex-1 overflow-hidden">
        {useCases.map((item) => (
          <div
            key={item.id}
            data-use-case-panel={item.id}
            className="gum-use-case-panel col-start-1 row-start-1 overflow-auto"
          >
              <div className="gum-code-body px-4 py-6 font-mono text-[13px] leading-7 text-zinc-200 md:px-6 md:text-sm text-left">
                <div className="space-y-0.5">
                  {item.lines.map((line, index) => (
                    <div
                      key={`${item.id}-${index}`}
                      className="gum-code-line grid grid-cols-[28px_minmax(0,1fr)] gap-3"
                    >
                      <span className="gum-code-gutter select-none text-right text-xs">
                        {index + 1}
                      </span>
                      <div className="text-left">{line || <span>&nbsp;</span>}</div>
                    </div>
                  ))}
                </div>
              </div>
          </div>
        ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
