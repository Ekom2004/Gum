import Link from "next/link";
import { SiteHeader } from "../../components/site-header";

export default function DocsPage() {
  return (
    <div className="min-h-screen bg-black text-zinc-200">
      <main className="mx-auto flex w-full max-w-[1360px] flex-col px-6 pt-28 pb-24 lg:px-8">
        <SiteHeader current="home" />

        <div className="mt-10 max-w-3xl">
          <span className="block text-[11px] font-medium uppercase tracking-[0.22em] text-zinc-500">
            Docs
          </span>
          <h1 className="gum-display mt-4 text-5xl font-bold leading-[0.96] tracking-tight text-white md:text-7xl">
            Gum Docs
          </h1>
          <p className="mt-5 max-w-2xl text-lg leading-relaxed text-zinc-400">
            The docs are still being consolidated. Start with the Python quickstart and the core job model.
          </p>
        </div>

        <div className="mt-12 grid max-w-3xl gap-4">
          <Link
            href="/"
            className="rounded-sm border border-zinc-800 px-5 py-4 text-zinc-100 transition-colors hover:border-zinc-700 hover:bg-zinc-950"
          >
            Back to homepage
          </Link>
          <Link
            href="/"
            className="rounded-sm border border-zinc-800 px-5 py-4 text-zinc-100 transition-colors hover:border-zinc-700 hover:bg-zinc-950"
          >
            Core job model
          </Link>
        </div>
      </main>
    </div>
  );
}
