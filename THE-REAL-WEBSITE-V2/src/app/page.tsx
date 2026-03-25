"use client";

import { ArrowRight, Check, Copy } from "lucide-react";
import React from "react";
import { SiteHeader } from "../components/site-header";

const INSTALL_COMMAND = "pip install mx8";

export default function Home() {
  const [copied, setCopied] = React.useState(false);

  const handleCopyInstall = async () => {
    try {
      await navigator.clipboard.writeText(INSTALL_COMMAND);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="relative min-h-screen overflow-hidden bg-[#09090b] font-sans text-zinc-300 selection:bg-zinc-200 selection:text-zinc-950">
      {/* Subtle Grid Background */}
      <div className="absolute inset-0 bg-[linear-gradient(to_right,#27272a1a_1px,transparent_1px),linear-gradient(to_bottom,#27272a1a_1px,transparent_1px)] bg-[size:24px_24px] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_100%)]"></div>

      <main className="relative z-10 flex flex-col items-center justify-center pt-32 pb-32 px-6 max-w-5xl mx-auto">
        
        <SiteHeader current="home" />
        
        {/* Hero Section */}
        <div className="text-center max-w-3xl flex flex-col items-center gap-6 mt-12">
          <div className="mb-4 inline-flex items-center gap-2 rounded-full border border-zinc-800 bg-zinc-900/80 px-3 py-1 text-xs text-zinc-400">
            <span className="flex h-2 w-2 rounded-full bg-emerald-400 animate-pulse"></span>
            v0.2.0 API is live
          </div>
          
          <h1 className="font-[family:var(--font-heading)] text-5xl md:text-7xl font-bold tracking-tight text-white leading-tight">
            Search and transform <br className="hidden md:block" />
            <span className="text-zinc-500">massive media datasets.</span>
          </h1>
          
          <p className="mt-4 max-w-2xl text-lg leading-relaxed text-zinc-400 md:text-xl">
            Point MX8 at your media, define the job, and get finished outputs back.
          </p>

          <div className="flex flex-wrap gap-4 mt-8 items-center justify-center">
            <button className="inline-flex items-center gap-2 rounded-[6px] bg-white px-6 py-3 font-semibold text-black transition-colors hover:bg-zinc-200">
              Read Documentation
              <ArrowRight size={16} />
            </button>
            <button className="rounded-[6px] border border-zinc-800 bg-zinc-900/90 px-6 py-3 font-semibold text-zinc-100 transition-colors hover:border-zinc-700 hover:bg-zinc-800">
              Book Demo
            </button>
          </div>
        </div>

        <div className="mt-20 w-full max-w-3xl">
          <div className="mb-4 overflow-hidden rounded-lg border border-zinc-800 bg-zinc-950/90 shadow-[0_0_0_1px_rgba(39,39,42,0.45)]">
            <div className="flex items-center justify-between border-b border-zinc-800 bg-zinc-900/90 px-4 py-2">
              <span className="text-[11px] font-medium uppercase tracking-[0.22em] text-zinc-500">
                Install
              </span>
              <button
                type="button"
                onClick={handleCopyInstall}
                className="inline-flex appearance-none items-center gap-2 rounded-[6px] border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs font-medium uppercase tracking-[0.18em] text-zinc-400 transition-colors hover:border-zinc-700 hover:text-zinc-100"
                title="Copy install command"
              >
                {copied ? <Check size={14} /> : <Copy size={14} />}
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
            <div className="flex items-center gap-3 px-5 py-4">
              <span className="font-mono text-sm text-zinc-500">$</span>
              <code className="truncate font-mono text-sm tracking-wide text-zinc-100 md:text-base">
                {INSTALL_COMMAND}
              </code>
            </div>
          </div>

          {/* The Code Editor Window */}
          <div className="w-full overflow-hidden rounded-lg border border-zinc-800 bg-black/90 shadow-2xl backdrop-blur-sm">
            <div className="flex items-center border-b border-zinc-800 bg-zinc-950/90 px-4 py-3">
              <span className="text-[11px] font-medium uppercase tracking-[0.22em] text-zinc-500">
                Example
              </span>
            </div>
            <div className="overflow-x-auto p-6 font-mono text-sm leading-relaxed md:text-base">
              <pre className="text-[#d4d4d4]">
                <code>
<span className="text-[#c586c0]">import</span> <span className="text-[#d4d4d4]">mx8</span><br/>
<br/>
<span className="text-[#6a9955]"># Find and transform petabytes of video instantly</span><br/>
<span className="text-[#d4d4d4]">mx8</span>.<span className="text-[#dcdcaa]">run</span>(<br/>
{"    "}<span className="text-[#9cdcfe]">input</span>=<span className="text-[#ce9178]">"s3://raw-dashcam-archive/"</span>,<br/>
{"    "}<span className="text-[#9cdcfe]">work</span>=[<br/>
{"        "}<span className="text-[#d4d4d4]">mx8</span>.<span className="text-[#dcdcaa]">find</span>(<span className="text-[#ce9178]">"a stop sign covered in heavy snow"</span>),<br/>
{"        "}<span className="text-[#d4d4d4]">mx8</span>.<span className="text-[#dcdcaa]">extract_frames</span>(<span className="text-[#9cdcfe]">fps</span>=<span className="text-[#b5cea8]">10</span>, <span className="text-[#9cdcfe]">format</span>=<span className="text-[#ce9178]">"jpg"</span>),<br/>
{"    "}],<br/>
{"    "}<span className="text-[#9cdcfe]">output</span>=<span className="text-[#ce9178]">"s3://training-dataset/"</span><br/>
)
                </code>
              </pre>
            </div>
          </div>
        </div>

        <div className="mt-20 w-full border-t border-zinc-800">
          <div className="grid grid-cols-1 border-b border-zinc-800 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 md:border-b-0 md:border-r" />
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              <span className="block text-base font-medium text-zinc-100">How do I run it?</span>
              <span className="mt-2 block">
                Point MX8 at the input, define the work, and it handles the execution path behind the scenes.
              </span>
            </div>
          </div>
          <div className="grid grid-cols-1 border-b border-zinc-800 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 md:border-b-0 md:border-r" />
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              <span className="block text-base font-medium text-zinc-100">What do I get back?</span>
              <span className="mt-2 block">
                Finished outputs, ready to use wherever you need them.
              </span>
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-[240px_1fr]">
            <div className="border-b border-zinc-800 md:border-b-0 md:border-r" />
            <div className="px-2 py-5 text-sm leading-relaxed text-zinc-400 md:px-6">
              <span className="block text-base font-medium text-zinc-100">Do I need to move my archive first?</span>
              <span className="mt-2 block">
                No. MX8 is built to run where your media already lives, from object storage to edge-connected sources.
              </span>
            </div>
          </div>
        </div>

      </main>
    </div>
  );
}
