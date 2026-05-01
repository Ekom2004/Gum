import type { Metadata } from 'next';
import { Geist, Geist_Mono } from 'next/font/google';
import './globals.css';

export const metadata: Metadata = {
  title: 'Gum | Background Jobs That Do Not Break',
  description: 'Schedule it. Retry it. Bound it. Replay it. Write the function and let Gum run it.',
};

const geistSans = Geist({
  subsets: ['latin'],
  variable: '--font-geist-sans',
  display: 'swap',
});

const geistMono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-geist-mono',
  display: 'swap',
});

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body
        className={`${geistSans.variable} ${geistMono.variable} text-white selection:bg-zinc-800 selection:text-white`}
        style={
          {
            "--font-heading": 'var(--font-geist-sans), "Satoshi", "Plus Jakarta Sans", sans-serif',
            "--font-body": 'var(--font-geist-sans), "Satoshi", "Plus Jakarta Sans", sans-serif',
            "--font-sans": 'var(--font-geist-sans), "Satoshi", "Plus Jakarta Sans", sans-serif',
            "--font-mono": 'var(--font-geist-mono), "JetBrains Mono", "Fira Code", monospace',
          } as React.CSSProperties
        }
      >
        {children}
      </body>
    </html>
  );
}
