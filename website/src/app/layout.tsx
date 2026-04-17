import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'Gum | Background Jobs That Do Not Break',
  description: 'Schedule it. Retry it. Bound it. Replay it. Write the function and let Gum run it.',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body
        className="bg-black text-white selection:bg-zinc-800 selection:text-white"
        style={
          {
            "--font-heading":
              '"IBM Plex Sans Condensed", "IBM Plex Sans", "Arial Narrow", sans-serif',
            "--font-body":
              '"IBM Plex Sans", "Helvetica Neue", Helvetica, Arial, sans-serif',
          } as React.CSSProperties
        }
      >
        {children}
      </body>
    </html>
  );
}
