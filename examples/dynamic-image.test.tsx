import fs from 'node:fs'
import React from 'react'
import { afterEach, expect, it, vi } from 'vitest'
import { createTestRoot, hasNativeTestRenderer } from '@gpuix/react/testing'
import { DynamicImage } from './dynamic-image'

const itNative = hasNativeTestRenderer ? it : it.skip

afterEach(() => vi.useRealTimers())

itNative('animates one img without React commits and stops on unmount', () => {
  vi.useFakeTimers()
  const root = createTestRoot({ width: 304, height: 304 })
  const uploads = vi.spyOn(root.renderer, 'updateImage')
  const commits = vi.spyOn(root.renderer, 'applyBatch')
  root.render(<DynamicImage />)
  const committed = commits.mock.calls.length
  fs.mkdirSync('screenshots', { recursive: true })
  root.renderer.captureScreenshot('screenshots/dynamic-image-first.png')
  const first = fs.readFileSync('screenshots/dynamic-image-first.png')
  vi.advanceTimersByTime(200)
  root.renderer.captureScreenshot('screenshots/dynamic-image-animated.png')
  expect(fs.readFileSync('screenshots/dynamic-image-animated.png')).not.toEqual(first)
  expect(root.renderer.findByType('img')).toHaveLength(1)
  expect(uploads).toHaveBeenCalledTimes(6)
  expect(commits).toHaveBeenCalledTimes(committed)
  root.unmount()
  vi.advanceTimersByTime(200)
  expect(uploads).toHaveBeenCalledTimes(6)
})
