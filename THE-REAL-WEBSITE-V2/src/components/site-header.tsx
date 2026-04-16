import Link from "next/link";

type SiteHeaderProps = {
  current?: "home" | "book-demo";
};

export function SiteHeader({ current = "home" }: SiteHeaderProps) {
  return (
    <div className="absolute inset-x-0 top-11 lg:top-[3.25rem]">
      <div className="mx-auto flex w-full justify-center px-6 lg:px-8">
        <div className="grid w-full max-w-[1200px] lg:w-fit lg:grid-cols-[540px_620px] lg:gap-10">
          <Link
            href="/"
            className="flex items-center text-[21px] font-semibold tracking-[0.24em] text-[var(--gum-paper)]"
          >
            GUM
          </Link>
        </div>
      </div>
    </div>
  );
}
