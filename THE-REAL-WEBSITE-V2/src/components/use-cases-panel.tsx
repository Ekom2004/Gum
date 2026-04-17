"use client";

import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState, type ReactNode } from "react";

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
  const [activeId, setActiveId] = useState<UseCase["id"]>("webhooks");
  const active = useCases.find((item) => item.id === activeId) ?? useCases[0];
  const tabRefs = useRef<Partial<Record<UseCase["id"], HTMLButtonElement | null>>>({});

  useEffect(() => {
    tabRefs.current[activeId]?.scrollIntoView({
      behavior: "smooth",
      block: "nearest",
      inline: "center",
    });
  }, [activeId]);

  return (
    <div className="w-full max-w-[980px]">
      <div className="relative">
        <div className="pointer-events-none absolute inset-y-0 left-0 z-10 w-8 bg-gradient-to-r from-[#09090b] to-transparent lg:hidden" />
        <div className="pointer-events-none absolute inset-y-0 right-0 z-10 w-8 bg-gradient-to-l from-[#09090b] to-transparent lg:hidden" />
        <div className="gum-scrollbar-none overflow-x-auto scroll-smooth px-1 lg:overflow-visible lg:px-0">
          <div className="flex min-w-max gap-4 border-b border-zinc-900/90 pb-4 lg:min-w-0 lg:grid lg:grid-cols-4 lg:gap-4">
            {useCases.map((item) => {
              const isActive = item.id === active.id;
              return (
                <button
                  key={item.id}
                  ref={(node) => {
                    tabRefs.current[item.id] = node;
                  }}
                  type="button"
                  onClick={() => setActiveId(item.id)}
                  className={`relative snap-start whitespace-nowrap border-b pb-2 text-left text-[13px] font-medium tracking-[-0.01em] transition-colors lg:w-full ${
                    isActive
                      ? "border-zinc-300 text-zinc-100"
                      : "border-transparent text-zinc-500 hover:text-zinc-300"
                  }`}
                >
                  {item.tabLabel}
                  {isActive ? (
                    <motion.span
                      layoutId="gum-use-case-active-line"
                      className="absolute inset-x-0 -bottom-px h-px bg-zinc-200"
                      transition={{ type: "spring", stiffness: 420, damping: 36 }}
                    />
                  ) : null}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <AnimatePresence mode="wait" initial={false}>
        <motion.div
          key={active.id}
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -8 }}
          transition={{ duration: 0.22, ease: "easeOut" }}
          className="pt-5"
        >
          <div>
            <p className="max-w-[42rem] text-sm leading-relaxed text-zinc-400">
              {active.description}
            </p>
          </div>
          <motion.div
            initial={{ opacity: 0.92, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.2, ease: "easeOut", delay: 0.03 }}
            className="gum-code-surface relative mt-5 w-full overflow-hidden rounded-sm border border-zinc-800 bg-black text-left"
          >
            <div className="min-h-[320px] bg-black px-4 py-5 font-mono text-[13px] leading-7 text-[#e4e4e7] md:text-sm text-left">
              <div className="space-y-0.5">
                {active.lines.map((line, index) => (
                  <div
                    key={`${active.id}-${index}`}
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
          </motion.div>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
