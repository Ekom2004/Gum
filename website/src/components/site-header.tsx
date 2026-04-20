import Link from "next/link";

type SiteHeaderProps = {
  current?: "home" | "book-demo";
};

export function SiteHeader({ current = "home" }: SiteHeaderProps) {
  return (
    <div className="gum-site-header absolute inset-x-0 top-0 border-b border-zinc-800">
      <div className="mx-auto flex w-full justify-center px-6 py-5 lg:px-8 lg:py-6">
        <div className="grid w-full max-w-[1200px] lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
          <Link
            href="/"
            className="flex items-end pb-0.5 text-[20px] font-semibold tracking-[0.22em] text-zinc-50"
          >
            GUM
          </Link>
        </div>
      </div>
    </div>
  );
}
