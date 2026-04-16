"use client";

import { useState, type ReactNode } from "react";

type UseCase = {
  id: "webhooks" | "crm" | "emails" | "uploads" | "rate_limited";
  label: string;
  description: string;
  note: string;
  runtimeLabel: string;
  runtimeSummary: string;
  logLine: string;
  lines: ReactNode[];
};

const useCases: UseCase[] = [
  {
    id: "webhooks",
    label: "Retry failed webhooks",
    description: "Catch flaky downstream calls and let Gum retry failed webhook delivery.",
    note: "Retries happen on the job, not in a retry table you maintain yourself.",
    runtimeLabel: "Recovered on retry",
    runtimeSummary: "Attempt 1 failed with 503, attempt 2 delivered successfully.",
    logLine: "POST partner.acme.com/webhooks -> 200 OK after retry",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">httpx</span>
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
        <span className="text-[#ce9178]">&quot;2m&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        <span className="text-[#c586c0]">def</span>{" "}
        <span className="text-[#dcdcaa]">deliver_webhook</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">event_id</span>
        <span className="text-zinc-300">: </span>
        <span className="text-[#4ec9b0]">str</span>
        <span className="text-zinc-300">):</span>
      </>,
      <>
        {"    "}
        <span className="text-[#9cdcfe]">payload</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#dcdcaa]">load_event_payload</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">event_id</span>
        <span className="text-zinc-300">)</span>
      </>,
      <>
        {"    "}
        <span className="text-[#9cdcfe]">httpx</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">post</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#ce9178]">&quot;https://partner.acme.com/webhooks&quot;</span>
        <span className="text-zinc-300">, </span>
        <span className="text-[#9cdcfe]">json</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#9cdcfe]">payload</span>
        <span className="text-zinc-300">)</span>
      </>,
      "",
      <>
        <span className="text-[#dcdcaa]">deliver_webhook</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">enqueue</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">event_id</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;evt_123&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "crm",
    label: "Sync new signups to a CRM",
    description: "Push signup events into HubSpot or Salesforce without owning worker glue.",
    note: "A normal enqueue-on-signup job.",
    runtimeLabel: "Queued from signup",
    runtimeSummary: "The app enqueues once, Gum handles the rest in the background.",
    logLine: "upserted usr_123 -> lifecycle_stage=lead",
    lines: [
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">gum</span>
      </>,
      <>
        <span className="text-[#c586c0]">import</span>{" "}
        <span className="text-[#9cdcfe]">hubspot</span>
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
        <span className="text-zinc-300">)</span>
      </>,
      <>
        <span className="text-[#c586c0]">def</span>{" "}
        <span className="text-[#dcdcaa]">sync_signup</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">user_id</span>
        <span className="text-zinc-300">: </span>
        <span className="text-[#4ec9b0]">str</span>
        <span className="text-zinc-300">):</span>
      </>,
      <>
        {"    "}
        <span className="text-[#9cdcfe]">hubspot</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">create_or_update_contact</span>
        <span className="text-zinc-300">(</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">user_id</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">lifecycle_stage</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;lead&quot;</span>
        <span className="text-zinc-300">,</span>
      </>,
      <>
        {"        "}
        <span className="text-[#9cdcfe]">source</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;signup&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
      "",
      <>
        <span className="text-[#dcdcaa]">sync_signup</span>
        <span className="text-zinc-300">.</span>
        <span className="text-[#dcdcaa]">enqueue</span>
        <span className="text-zinc-300">(</span>
        <span className="text-[#9cdcfe]">user_id</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#ce9178]">&quot;usr_123&quot;</span>
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "emails",
    label: "Send emails later or on a schedule",
    description: "Queue follow-ups now or let Gum run lifecycle emails on a schedule.",
    note: "Scheduled jobs are built in.",
    runtimeLabel: "Scheduled automatically",
    runtimeSummary: "No cron glue, no separate scheduler config in your app.",
    logLine: "scheduled run fired at 2026-04-15 09:00 UTC",
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
        <span className="text-zinc-300">)</span>
      </>,
    ],
  },
  {
    id: "uploads",
    label: "Process uploads in the background",
    description: "Move file-heavy work off the request path and let Gum own retries and timeouts.",
    note: "Bounded long-running work is still just a job.",
    runtimeLabel: "Long-running, still bounded",
    runtimeSummary: "Heavy file work moves off the request path without running forever.",
    logLine: "generated preview and stored search chunks for upl_123",
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
        <span className="text-[#ce9178]">&quot;20m&quot;</span>
        <span className="text-zinc-300">,</span>
        <span className="text-[#9cdcfe]">concurrency</span>
        <span className="text-zinc-500">=</span>
        <span className="text-[#b5cea8]">4</span>
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
    id: "rate_limited",
    label: "Call rate-limited APIs safely",
    description: "Keep third-party APIs under control with rate limits and bounded concurrency.",
    note: "One of Gum's strongest operational controls.",
    runtimeLabel: "Held under provider limits",
    runtimeSummary: "Gum paces execution so one burst does not melt the dependency.",
    logLine: "holding steady at 20 requests/minute across queued runs",
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
  const [activeId, setActiveId] = useState<UseCase["id"]>("webhooks");
  const active = useCases.find((item) => item.id === activeId) ?? useCases[0];

  return (
    <div className="grid w-full max-w-[980px] gap-8 lg:grid-cols-[272px_minmax(0,1fr)] lg:items-stretch lg:gap-8">
      <div className="flex flex-col pt-1 lg:min-h-[408px]">
        {useCases.map((item) => {
          const isActive = item.id === active.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveId(item.id)}
              className={`flex-1 border-l px-0 py-3 text-left transition-colors ${
                isActive
                  ? "border-zinc-100 bg-zinc-950/55 text-zinc-100"
                  : "border-zinc-900 text-zinc-500 hover:border-zinc-700 hover:text-zinc-300"
              }`}
            >
              <div
                className={`pl-5 pr-4 text-[15px] leading-[1.45] tracking-[-0.01em] ${
                  isActive ? "font-semibold" : "font-medium"
                }`}
              >
                {item.label}
              </div>
            </button>
          );
        })}
      </div>

      <div className="w-full max-w-[640px]">
        <div className="mb-4 text-[11px] font-medium uppercase tracking-[0.18em] text-zinc-500">
          {active.label}
        </div>
        <p className="mb-5 max-w-xl text-sm leading-relaxed text-zinc-400">{active.description}</p>
        <div className="gum-code-surface relative w-full overflow-hidden rounded-sm border border-zinc-800 bg-black text-left">
          <div className="min-h-[300px] bg-black px-4 py-5 font-mono text-[13px] leading-7 text-[#e4e4e7] md:text-sm text-left">
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
    </div>
  );
}
