import Link from "next/link";
import { HeroCodePanel } from "../components/hero-code-panel";
import { SiteHeader } from "../components/site-header";
import { UseCasesPanel } from "../components/use-cases-panel";

export default function Home() {
  return (
    <div className="relative min-h-screen overflow-hidden bg-black font-sans text-zinc-300 selection:bg-zinc-700 selection:text-zinc-50">
      <main className="relative z-10 mx-auto flex w-full max-w-[1360px] flex-col pt-20 pb-12 md:pb-16">
        <SiteHeader current="home" />

        <section className="relative mt-10">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_18%_24%,rgba(255,255,255,0.07),transparent_45%)]"
          />
          <div className="mx-auto flex w-full justify-center px-6 py-14 lg:px-8 lg:py-16">
            <div className="gum-container grid gap-12 lg:grid-cols-[480px_680px] lg:items-center lg:gap-16">
              <div className="flex flex-col items-start">
                <h1 className="gum-display max-w-[11ch] text-[3.2rem] font-medium leading-[0.98] tracking-[-0.03em] text-zinc-50 md:text-[4.55rem]">
                  <span className="text-zinc-400">Managed background jobs</span>
                  <br />
                  for Python
                </h1>

                <p className="mt-7 max-w-[42ch] text-[19px] leading-[1.6] text-zinc-400">
                  Run background jobs with retries, timeouts, rate limits, scheduling, and compute
                  allocation — without managing infrastructure.
                </p>

                <div className="mt-8 flex flex-wrap items-center gap-4">
                  <Link
                    href="/docs"
                    className="inline-flex h-11 items-center justify-center rounded-sm bg-zinc-50 px-6 font-mono text-[12px] uppercase tracking-[0.05em] text-black transition-colors hover:bg-zinc-200"
                  >
                    Start for free
                  </Link>
                  <Link
                    href="/docs"
                    className="inline-flex h-11 items-center justify-center rounded-sm border border-zinc-700 bg-zinc-950 px-6 font-mono text-[12px] uppercase tracking-[0.05em] text-zinc-100 transition-colors hover:bg-zinc-900"
                  >
                    Read docs →
                  </Link>
                </div>
              </div>
              <div className="flex w-full flex-col items-center lg:items-end">
                <HeroCodePanel />
              </div>
            </div>
          </div>
        </section>

        <section className="relative mt-12 md:mt-14">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute top-0 left-1/2 h-px w-screen -translate-x-1/2 bg-zinc-800"
          />
          <div className="mx-auto flex w-full justify-center px-6 lg:px-8">
            <div className="gum-container py-12 lg:py-14">
              <div className="mb-10 pt-5">
                <span className="block text-[11px] font-medium uppercase tracking-[0.2em] text-zinc-500">
                  Use Cases
                </span>
                <h2 className="gum-display mt-4 max-w-[15ch] text-3xl font-medium tracking-tight text-zinc-50 md:text-[2.6rem] md:leading-[1.06]">
                  Production work Gum handles.
                </h2>
                <p className="mt-6 max-w-2xl text-[19px] leading-8 text-zinc-400">
                  The work that usually turns into queues, cron jobs, workers, dashboards, and
                  late-night fixes.
                </p>
              </div>
              <UseCasesPanel />
            </div>
          </div>
        </section>

        <section className="relative mt-14 md:mt-16">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute top-0 left-1/2 h-px w-screen -translate-x-1/2 bg-zinc-800"
          />
          <div className="mx-auto flex w-full justify-center px-6 lg:px-8">
            <div className="gum-container py-12 lg:py-14">
              <div className="rounded-sm border border-zinc-800 bg-zinc-950 px-6 py-8 md:px-8 md:py-10">
                <span className="block text-[11px] font-medium uppercase tracking-[0.2em] text-zinc-500">
                  Next Step
                </span>
                <h2 className="gum-display mt-4 max-w-[14ch] text-3xl font-medium tracking-tight text-zinc-50 md:text-[2.6rem] md:leading-[1.06]">
                  Stop babysitting job infrastructure.
                </h2>
                <p className="mt-5 max-w-[58ch] text-[19px] leading-8 text-zinc-400">
                  Deploy one function, enqueue one real job, and watch retries, logs, and
                  execution status from one place.
                </p>
                <div className="mt-8 flex flex-wrap items-center gap-4">
                  <Link
                    href="/docs"
                    className="inline-flex h-11 items-center justify-center rounded-sm bg-zinc-50 px-6 font-mono text-[12px] uppercase tracking-[0.05em] text-black transition-colors hover:bg-zinc-200"
                  >
                    Start building
                  </Link>
                  <Link
                    href="/book-demo"
                    className="inline-flex h-11 items-center justify-center rounded-sm border border-zinc-700 bg-zinc-950 px-6 font-mono text-[12px] uppercase tracking-[0.05em] text-zinc-100 transition-colors hover:bg-zinc-900"
                  >
                    Book a demo
                  </Link>
                  <Link
                    href="/docs"
                    className="inline-flex h-11 items-center justify-center rounded-sm border border-zinc-700 px-4 font-mono text-[12px] uppercase tracking-[0.05em] text-zinc-100 transition-colors hover:bg-zinc-900/60"
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
