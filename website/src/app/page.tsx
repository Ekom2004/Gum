import Link from "next/link";
import { HeroCodePanel } from "../components/hero-code-panel";
import { SiteHeader } from "../components/site-header";
import { UseCasesPanel } from "../components/use-cases-panel";

const featureCards = [
  {
    title: "Concurrency",
    description: "Limit execution slots automatically without leaking capacity.",
  },
  {
    title: "Health checks",
    description: "Hold work when a function or provider starts failing.",
  },
  {
    title: "Deduplication",
    description: "Use keys to drop duplicate jobs before they run twice.",
  },
];

export default function Home() {
  return (
    <div className="gum-site-shell relative min-h-screen overflow-hidden bg-black font-sans text-zinc-300 selection:bg-zinc-200 selection:text-zinc-950">
      <main className="relative z-10 mx-auto flex w-full max-w-[1360px] flex-col pt-28 pb-24">
        <SiteHeader current="home" />

        <section className="relative mt-8">
          <div className="mx-auto flex w-full justify-center px-6 py-16 lg:px-8 lg:py-20">
            <div className="grid w-full max-w-[1200px] gap-16 lg:w-fit lg:grid-cols-[540px_620px] lg:items-center lg:gap-16">
              <div className="flex flex-col items-center text-center lg:self-stretch lg:items-start lg:justify-center lg:text-left">
                <div className="gum-eyebrow mb-4 inline-flex w-fit items-center gap-2 rounded-md border border-zinc-800 bg-zinc-950 px-3 py-1 text-[11px] font-medium uppercase tracking-[0.2em] text-zinc-500">
                  <span className="relative flex h-2 w-2">
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-40"></span>
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400"></span>
                  </span>
                  Early access open
                </div>
                <h1 className="gum-display mb-6 max-w-[18ch] text-5xl font-medium leading-[1.1] tracking-tight text-white md:text-6xl">
                  Managed functions, with reliability
                  <br />
                  built in.
                </h1>

                <p className="mb-10 max-w-2xl text-lg leading-relaxed text-zinc-400">
                  Stop writing Terraform for SQS queues and managing Redis locks. Gum is a background
                  runtime that executes your code with retries, timeouts, global rate limits, and
                  deduplication natively injected.
                </p>

                <div className="flex flex-wrap items-center justify-center gap-4 lg:justify-start">
                  <button className="gum-primary inline-flex h-11 items-center justify-center gap-3 rounded-md bg-white px-6 py-3 text-sm font-medium text-black transition-colors hover:bg-zinc-200">
                    <span>Start building</span>
                    <span aria-hidden="true" className="text-[0.95em] text-zinc-600">
                      →
                    </span>
                  </button>
                  <Link
                    href="/book-demo"
                    className="inline-flex h-11 items-center justify-center rounded-md bg-zinc-900 px-6 py-3 text-sm font-medium text-white transition-colors hover:bg-zinc-800"
                  >
                    Book a demo
                  </Link>
                </div>
              </div>
              <div className="flex w-full justify-center lg:justify-end">
                <HeroCodePanel />
              </div>
            </div>
          </div>
        </section>

        <section className="mt-32 md:mt-48">
          <div className="mx-auto flex w-full justify-center px-6 lg:px-8">
            <div className="w-full max-w-[1200px]">
              <div className="grid grid-cols-1 gap-px overflow-hidden rounded-lg border border-zinc-800 bg-zinc-800 md:grid-cols-3">
                {featureCards.map((feature) => (
                  <div key={feature.title} className="flex flex-col bg-black p-8 md:p-12">
                    <h3 className="mb-2 text-lg font-medium text-white">{feature.title}</h3>
                    <p className="text-sm leading-relaxed text-zinc-400">{feature.description}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>

        <section className="mt-32 md:mt-48">
          <div className="mx-auto w-full max-w-[1200px] px-6 py-16 lg:px-8 lg:py-20">
            <div className="mb-12 border-t border-zinc-800 pt-8">
              <span className="block text-[11px] font-medium uppercase tracking-[0.2em] text-zinc-500">
                Use Cases
              </span>
              <h2 className="gum-display mt-4 max-w-[15ch] text-3xl font-medium tracking-tight text-white md:text-4xl">
                Production work Gum handles.
              </h2>
              <p className="mt-6 max-w-2xl text-lg leading-relaxed text-zinc-400">
                The work that usually turns into queues, cron jobs, workers, dashboards, and
                late-night fixes.
              </p>
            </div>
            <UseCasesPanel />
          </div>
        </section>

        <section className="mt-32 md:mt-48">
          <div className="mx-auto flex w-full max-w-[1200px] px-6 py-16 lg:px-8 lg:py-20">
            <div className="w-full border-t border-zinc-800 pt-8">
              <h2 className="gum-display max-w-[14ch] text-3xl font-medium tracking-tight text-white">
                Stop babysitting job infrastructure.
              </h2>
              <p className="mt-6 max-w-2xl text-lg leading-relaxed text-zinc-400">
                Write the function. Gum runs it reliably.
              </p>
              <div className="mt-10 flex flex-wrap gap-4">
                <button className="gum-primary inline-flex h-11 items-center gap-3 rounded-md bg-white px-6 py-3 text-sm font-medium text-black transition-colors hover:bg-zinc-200">
                  <span>Start building</span>
                  <span aria-hidden="true" className="text-[0.95em] text-zinc-600">
                    →
                  </span>
                </button>
                <Link
                  href="/book-demo"
                  className="inline-flex h-11 items-center rounded-md bg-zinc-900 px-6 py-3 text-sm font-medium text-white transition-colors hover:bg-zinc-800"
                >
                  Book a demo
                </Link>
                <Link
                  href="/docs"
                  className="inline-flex items-center gap-2 rounded-md border border-zinc-800 bg-transparent px-6 py-3 text-sm font-medium text-zinc-300 transition-colors hover:bg-zinc-900"
                >
                  Read docs
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}
