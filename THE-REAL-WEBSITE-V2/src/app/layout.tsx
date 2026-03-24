import type { Metadata } from 'next';
import { IBM_Plex_Sans, IBM_Plex_Sans_Condensed } from 'next/font/google';
import './globals.css';

const ibmPlexSans = IBM_Plex_Sans({
  subsets: ['latin'],
  weight: ['400', '500', '600', '700'],
});

const ibmPlexSansCondensed = IBM_Plex_Sans_Condensed({
  subsets: ['latin'],
  weight: ['600', '700'],
  variable: '--font-heading',
});

export const metadata: Metadata = {
  title: 'MX8 | Media Transforms at Scale',
  description: 'Point MX8 at your data, define the transform, and get transformed outputs where you need them.',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body className={`${ibmPlexSans.className} ${ibmPlexSansCondensed.variable} bg-[#09090b] text-white selection:bg-zinc-800 selection:text-white`}>
        {children}
      </body>
    </html>
  );
}
