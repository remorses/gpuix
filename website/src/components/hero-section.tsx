'use client'

import { preload } from 'react-dom'
import { VideoBackgroundShader } from '@holocron.so/vite/mdx'

const HERO_FONT = "'ArizonaFlare', serif"

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox='0 0 24 24' fill='currentColor'>
      <path d='M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z' />
    </svg>
  )
}

export function HeroSection({
  lines = ['GPUI and React', 'for native apps'],
}: {
  lines?: [string, string]
}) {
  // Starts the font download from <head>. `font-display: block` in globals.css keeps
  // the title hidden until it arrives, so no JS gate (which delayed LCP to hydration).
  preload('/fonts/arizona-flare-regular.woff2', { as: 'font', type: 'font/woff2', crossOrigin: '' })

  return (
    <div className='relative mt-2 lg:mt-4 mb-4 lg:mb-6 w-full overflow-hidden'>
      <VideoBackgroundShader
        src='/hero-bg.mp4'
        className='absolute inset-0 w-full h-full'
        dotStyle='dots'
        dotColor='#a1cef8'
        dotAlphaMultiplier={0.7}
        dotSize={6}
      />

      <div className='relative z-[2] flex flex-col items-center justify-center text-center w-full px-5 py-12 sm:py-16 lg:py-20 gap-4'>
        <h1
          className='flex flex-col items-center leading-none tracking-tight text-[40px] sm:text-[56px] md:text-[68px] text-foreground'
          style={{ fontFamily: HERO_FONT }}
        >
          <span>{lines[0]}</span>
          <span>{lines[1]}</span>
        </h1>

        <div className='flex gap-2 flex-wrap justify-center'>
          <a
            href='https://github.com/remorses/gpuix'
            target='_blank'
            rel='noopener noreferrer'
            className='inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground no-underline hover:bg-primary/90'
          >
            <GitHubIcon className='size-3.5' />
            GitHub Repo
          </a>
          <a
            href='/chat-example/'
            className='inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-3 text-xs font-medium text-foreground no-underline hover:bg-accent'
          >
            Try in browser
          </a>
        </div>
      </div>
    </div>
  )
}
