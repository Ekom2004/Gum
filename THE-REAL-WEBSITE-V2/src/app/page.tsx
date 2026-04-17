import Link from "next/link";
import { HeroCodePanel } from "../components/hero-code-panel";
import { SiteHeader } from "../components/site-header";
import { UseCasesPanel } from "../components/use-cases-panel";

export default function Home() {
  return (
    <div className="relative min-h-screen overflow-hidden bg-black font-sans text-zinc-300 selection:bg-zinc-200 selection:text-zinc-950">
      <main className="relative z-10 mx-auto flex w-full max-w-[1360px] flex-col px-6 pt-28 pb-24 lg:px-8">
        <SiteHeader current="home" />

        <section className="relative mt-6">
          <div className="mx-auto flex w-full justify-center px-6 py-10 lg:px-8 lg:py-12">
            <div className="grid w-full max-w-[1200px] gap-10 lg:w-fit lg:grid-cols-[540px_620px] lg:items-center lg:gap-10">
              <div className="flex flex-col items-center gap-3.5 text-center lg:self-stretch lg:items-start lg:justify-center lg:text-left">
                <div className="inline-flex w-fit items-center gap-2 rounded-sm border border-zinc-800 bg-zinc-900/80 px-3 py-1 text-xs text-zinc-400">
                  <span className="flex h-2 w-2 rounded-full bg-emerald-400 animate-pulse"></span>
                  Early access open
                </div>
                <h1 className="max-w-[15.5ch] font-[family:var(--font-heading)] text-4xl font-bold leading-[1.01] tracking-[-0.03em] gum-paper-text md:text-6xl xl:text-[4.65rem]">
                  Deploy production functions on Gum.
                </h1>

                <p className="max-w-xl text-base leading-relaxed text-zinc-400 md:text-[1.05rem]">
                  Retries, timeouts, rate limits, concurrency, and scheduling built in. Write the
                  function. Gum runs it for you.
                </p>

                <div className="mt-2 flex flex-wrap items-center justify-center gap-3 lg:justify-start">
                  <button className="gum-primary inline-flex min-w-[224px] items-center justify-center gap-3 rounded-sm px-9 py-3.5 text-[15px] font-semibold tracking-[-0.01em] transition-colors md:min-w-[248px] md:px-11 md:py-4 md:text-base">
                    <span>Start building</span>
                    <span aria-hidden="true" className="text-[0.95em] text-zinc-700">
                      →
                    </span>
                  </button>
                </div>
                <div className="gum-meta-grid mt-4">
                  <div className="gum-meta-cell">
                    <span className="gum-meta-label">Compute</span>
                    <span className="gum-meta-value">Runs on Gum infrastructure</span>
                  </div>
                  <div className="gum-meta-cell">
                    <span className="gum-meta-label">Scaling</span>
                    <span className="gum-meta-value">Autoscaled with demand</span>
                  </div>
                  <div className="gum-meta-cell">
                    <span className="gum-meta-label">Execution</span>
                    <span className="gum-meta-value">Enqueue or schedule</span>
                  </div>
                </div>
              </div>
              <div className="flex w-full justify-center lg:justify-end">
                <HeroCodePanel />
              </div>
            </div>
          </div>
        </section>

        <div aria-hidden="true" className="gum-section-divider mt-16" />

        <section className="mt-18">
          <div className="mx-auto flex w-full justify-center px-6 py-10 lg:px-8 lg:py-12">
            <div className="grid w-full max-w-[1200px] gap-10 lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
              <div className="gum-module w-full max-w-[980px] px-6 py-6 lg:col-span-2 lg:px-8 lg:py-8">
                <span className="block text-[11px] font-medium uppercase tracking-[0.22em] text-zinc-500">
                  Use Cases
                </span>
                <div className="mt-4 max-w-[36rem]">
                  <h2 className="gum-paper-text font-[family:var(--font-heading)] text-3xl font-bold leading-[1.04] tracking-tight md:text-5xl">
                    Production work Gum handles.
                  </h2>
                </div>
                <div className="mt-8 w-full border-t border-zinc-900/90 pt-7">
                  <UseCasesPanel />
                </div>
              </div>
            </div>
          </div>
        </section>

        <div aria-hidden="true" className="gum-section-divider mt-16" />

        <section className="mt-18">
          <div className="mx-auto flex w-full justify-center px-6 py-10 lg:px-8 lg:py-12">
            <div className="grid w-full max-w-[1200px] gap-10 lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
              <div className="gum-module w-full max-w-[980px] px-6 py-6 lg:col-span-2 lg:px-8 lg:py-8">
                <h2 className="gum-paper-text max-w-[14ch] font-[family:var(--font-heading)] text-3xl font-bold leading-[1.02] tracking-tight md:text-[2.75rem]">
                  Stop babysitting job infrastructure.
                </h2>
                <p className="mt-4 max-w-[36rem] text-base leading-relaxed text-zinc-400 md:text-[1.05rem]">
                  Write the function. Gum runs it reliably.
                </p>
                <div className="mt-6 flex flex-wrap gap-3">
                  <button className="gum-primary inline-flex items-center gap-3 rounded-sm px-8 py-3.5 text-[15px] font-semibold tracking-[-0.01em] transition-colors md:text-base">
                    <span>Start building</span>
                    <span aria-hidden="true" className="text-[0.95em] text-zinc-700">
                      →
                    </span>
                  </button>
                  <Link
                    href="/docs"
                    className="gum-paper-text inline-flex items-center gap-2 rounded-sm border border-zinc-800 bg-transparent px-7 py-3.5 text-[15px] font-semibold tracking-[-0.01em] transition-colors hover:border-zinc-700 hover:bg-zinc-950 md:text-base"
                  >
                    Read docs
                  </Link>
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}
