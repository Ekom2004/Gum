import type { Metadata } from "next";

import { SiteHeader } from "../../components/site-header";

export const metadata: Metadata = {
  title: "About | MX8",
  description:
    "MX8 is building the runtime that makes unstructured data usable.",
};

export default function AboutPage() {
  return (
    <div className="relative min-h-screen overflow-hidden bg-[#09090b] text-zinc-300 selection:bg-zinc-200 selection:text-zinc-950">
      <div className="absolute inset-0 bg-[linear-gradient(to_right,#27272a1a_1px,transparent_1px),linear-gradient(to_bottom,#27272a1a_1px,transparent_1px)] bg-[size:24px_24px] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_100%)]" />

      <main className="relative z-10 mx-auto flex min-h-screen max-w-5xl flex-col px-6 pt-32 pb-24">
        <SiteHeader current="about" />

        <div className="mt-12 max-w-3xl">
          <div className="mb-6 inline-flex items-center gap-2 rounded-full border border-zinc-800 bg-zinc-900/80 px-3 py-1 text-xs text-zinc-400">
            About MX8
          </div>
          <h1 className="font-[family:var(--font-heading)] text-5xl font-bold leading-tight tracking-tight text-white md:text-7xl">
            We are building the runtime that makes unstructured data usable.
          </h1>
          <p className="mt-6 max-w-2xl text-lg leading-relaxed text-zinc-400 md:text-xl">
            Most systems work once the data is already structured. MX8 is for the harder
            part: turning raw video, images, audio, and other unstructured inputs into
            something teams can actually search, process, and use.
          </p>
        </div>

        <div className="mt-20 w-full border-t border-zinc-800">
          <div className="grid grid-cols-1 border-b border-zinc-800 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 px-2 py-5 text-[12px] font-semibold uppercase tracking-[0.18em] text-zinc-200 md:border-b-0 md:border-r md:px-0">
              What We Care About
            </div>
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              We care about taking data that is messy, high-volume, and hard to work with
              and turning it into something operational. Not just stored, but usable.
            </div>
          </div>
          <div className="grid grid-cols-1 border-b border-zinc-800 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 px-2 py-5 text-[12px] font-semibold uppercase tracking-[0.18em] text-zinc-200 md:border-b-0 md:border-r md:px-0">
              Why MX8 Exists
            </div>
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              Unstructured data has always been valuable, but most teams still do not have
              a clean way to search it, transform it, and move it into the rest of their
              workflow without building too much around the job itself.
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 px-2 py-5 text-[12px] font-semibold uppercase tracking-[0.18em] text-zinc-200 md:border-b-0 md:border-r md:px-0">
              How We Think
            </div>
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              The interface should stay high-level. The hard part should stay underneath.
              The point is not to expose more infrastructure. The point is to make
              unstructured data feel simple to use from the outside.
            </div>
          </div>
        </div>

        <div className="mt-20 max-w-3xl border-t border-zinc-800 pt-10">
          <p className="text-sm leading-relaxed text-zinc-500">
            We care about throughput, predictability, and product clarity more than hype.
            The goal is to make unstructured data usable without asking users to think
            about the machinery required to get there.
          </p>
        </div>
      </main>
    </div>
  );
}
