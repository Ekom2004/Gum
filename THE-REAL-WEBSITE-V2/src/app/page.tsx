import Link from "next/link";
import { HeroCodePanel } from "../components/hero-code-panel";
import { SiteHeader } from "../components/site-header";
import { UseCasesPanel } from "../components/use-cases-panel";

export default function Home() {
  return (
    <div className="relative min-h-screen overflow-hidden bg-black font-sans text-zinc-300 selection:bg-zinc-200 selection:text-zinc-950">
      <main className="relative z-10 mx-auto flex w-full max-w-[1360px] flex-col px-6 pt-24 pb-24 lg:px-8">
        <div
          aria-hidden="true"
          className="pointer-events-none absolute inset-6 hidden rounded-sm opacity-80 lg:block"
          style={{
            backgroundImage:
              "linear-gradient(to right, rgba(63,63,70,0.75) 0 10px, transparent 10px 24px), linear-gradient(to right, rgba(63,63,70,0.75) 0 10px, transparent 10px 24px), linear-gradient(to bottom, rgba(63,63,70,0.75) 0 10px, transparent 10px 24px), linear-gradient(to bottom, rgba(63,63,70,0.75) 0 10px, transparent 10px 24px)",
            backgroundSize: "24px 1px, 24px 1px, 1px 24px, 1px 24px",
            backgroundPosition: "top left, bottom left, top left, top right",
            backgroundRepeat: "repeat-x, repeat-x, repeat-y, repeat-y",
          }}
        />
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
                  Write functions that don&apos;t break in production.
                </h1>

                <p className="max-w-xl text-base leading-relaxed text-zinc-400 md:text-[1.05rem]">
                  Retries, timeouts, rate limits, concurrency, and scheduling built in. Write the
                  function. Let Gum manage it.
                </p>

                <div className="mt-2 flex flex-wrap items-center justify-center gap-3 lg:justify-start">
                  <button className="gum-primary inline-flex min-w-[224px] items-center justify-center gap-3 rounded-sm px-9 py-3.5 text-[15px] font-semibold tracking-[-0.01em] transition-colors md:min-w-[248px] md:px-11 md:py-4 md:text-base">
                    <span>Start building</span>
                    <span aria-hidden="true" className="text-[0.95em] text-zinc-700">
                      →
                    </span>
                  </button>
                </div>
              </div>
              <div className="flex w-full justify-center lg:justify-end">
                <HeroCodePanel />
              </div>
            </div>
          </div>
        </section>

        <div aria-hidden="true" className="gum-broken-divider mt-16" />

        <section className="mt-18">
          <div className="mx-auto flex w-full justify-center px-6 py-10 lg:px-8 lg:py-12">
            <div className="grid w-full max-w-[1200px] gap-10 lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
              <div className="w-full max-w-[980px] lg:col-span-2">
                <span className="block text-[11px] font-medium uppercase tracking-[0.22em] text-zinc-500">
                  Use Cases
                </span>
                <div className="mt-4 max-w-3xl">
                  <h2 className="gum-paper-text font-[family:var(--font-heading)] text-3xl font-bold leading-tight tracking-tight md:text-5xl">
                    One job model for the work every product ends up needing.
                  </h2>
                </div>
                <div className="mt-8 w-full">
                  <UseCasesPanel />
                </div>
              </div>
            </div>
          </div>
        </section>

        <div aria-hidden="true" className="gum-broken-divider mt-16" />

        <section className="mt-18">
          <div className="mx-auto flex w-full justify-center px-6 py-10 lg:px-8 lg:py-12">
            <div className="grid w-full max-w-[1200px] gap-10 lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
              <div className="w-full max-w-[980px] lg:col-span-2">
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
