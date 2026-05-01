"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

type SiteHeaderProps = {
  current?: "home" | "book-demo" | "docs" | "blog";
  theme?: "dark" | "day";
};

export function SiteHeader({ current = "home", theme = "dark" }: SiteHeaderProps) {
  const isDay = theme === "day";
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    const onScroll = () => {
      setCompact(window.scrollY > 8);
    };

    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <header
      className={`fixed inset-x-0 top-0 z-50 border-b transition-colors ${
        isDay ? "border-zinc-200 bg-zinc-50/95" : "border-zinc-800 bg-black/95"
      }`}
    >
      <div className="mx-auto flex w-full justify-center px-6 lg:px-8">
        <div
          className={`gum-container grid grid-cols-[auto_1fr_auto] items-center gap-5 transition-[height] duration-200 ${
            compact ? "h-14" : "h-16"
          }`}
        >
          <div className="flex items-center">
            <Link href="/" className="inline-flex items-center text-zinc-50">
              <span className="font-sans text-[20px] font-semibold tracking-[-0.025em]">gum</span>
            </Link>
          </div>

          <nav className="flex min-w-0 items-center justify-start gap-4 overflow-x-auto whitespace-nowrap gum-scrollbar-none md:justify-center md:gap-6">
            <Link
              href="/docs"
              className={`font-mono text-[12px] uppercase tracking-[0.12em] transition-colors ${
                current === "docs" ? "text-zinc-100" : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              Docs
            </Link>
            <Link
              href="/docs"
              className="font-mono text-[12px] uppercase tracking-[0.12em] text-zinc-500 transition-colors hover:text-zinc-300"
            >
              Use cases
            </Link>
            <Link
              href="/docs"
              className="font-mono text-[12px] uppercase tracking-[0.12em] text-zinc-500 transition-colors hover:text-zinc-300"
            >
              Pricing
            </Link>
            <Link
              href="/blog"
              className={`font-mono text-[12px] uppercase tracking-[0.12em] transition-colors ${
                current === "blog" ? "text-zinc-100" : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              Blog
            </Link>
          </nav>

          <div className="flex items-center justify-end gap-2">
            <Link
              href="/docs"
              className="inline-flex h-10 items-center justify-center rounded-sm bg-zinc-50 px-4 font-mono text-[11px] uppercase tracking-[0.06em] text-black transition-colors hover:bg-zinc-200"
            >
              Login
            </Link>
            <Link
              href="/book-demo"
              className="inline-flex h-10 items-center justify-center rounded-sm border border-zinc-700 bg-zinc-950 px-5 font-mono text-[12px] font-semibold uppercase tracking-[0.06em] text-zinc-100 transition-colors hover:bg-zinc-900"
            >
              Get in touch
            </Link>
          </div>
        </div>
      </div>
    </header>
  );
}
