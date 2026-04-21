import { redirect } from "next/navigation";

const docsUrl = process.env.NEXT_PUBLIC_GUM_DOCS_URL ?? "https://gum.mintlify.app";

export default function DocsPage() {
  redirect(docsUrl);
}
