/**
 * A video player built on Mediabunny, painted through a plain `<img>`.
 *
 * Run: cd examples && bun --hot video-player.tsx [file-or-url]
 *
 * Mediabunny demuxes the file and drives the decoders. Bun has no WebCodecs, so
 * `@mediabunny/server` registers FFmpeg decoders (NodeAV, hardware accelerated
 * where available) behind the same Mediabunny API a browser would use. Sound
 * goes out through `node-web-audio-api`, a Rust Web Audio implementation.
 *
 *   file / URL ─► Mediabunny Input ─┬─► VideoSampleSink ─► copyTo BGRA ─► <img>.setImagePixels
 *                                   └─► AudioSampleSink ─► AudioBuffer ─► AudioContext
 *
 * The AudioContext is the master clock, like Mediabunny's browser player
 * example. Audio buffers are scheduled at exact context times up to one second
 * ahead, and the video loop sleeps until each frame is due on that same clock,
 * dropping a frame that arrives too late. Without an audio track the context
 * still runs and keeps time.
 *
 * Each frame is copied to a CPU buffer and uploaded as a new GPUI image. There
 * is no zero-copy GPU video surface yet, see
 * https://github.com/remorses/gpuix/issues/92.
 */

import React, { useRef, useSyncExternalStore } from 'react'
import { render, useGpuix } from '@gpuix/react'
import type { EventPayload, ImgInstance, PublicInstance } from '@gpuix/react'
import {
  ALL_FORMATS,
  AudioSampleSink,
  FilePathSource,
  Input,
  UrlSource,
  VideoSampleSink,
} from 'mediabunny'
import type { AudioSample, VideoSample } from 'mediabunny'
import { registerMediabunnyServer } from '@mediabunny/server'
import { AudioBuffer, AudioContext } from 'node-web-audio-api'

registerMediabunnyServer()

// Sintel trailer (c) Blender Foundation, CC BY 3.0. Served with range requests,
// so Mediabunny streams it instead of downloading the whole file first.
const DEFAULT_SOURCE = 'https://download.blender.org/durian/trailer/sintel_trailer-720p.mp4'

// Bigger frames are scaled down before the copy. A 4K BGRA frame is 33 MB.
const MAX_FRAME_WIDTH = 1920
const MAX_FRAME_HEIGHT = 1080
// A frame this late is dropped so playback catches up with the clock.
const LATE_FRAME_MS = 40
// How far ahead of the clock audio is decoded and scheduled.
const AUDIO_AHEAD_SECONDS = 1
const SEEK_STEP_SECONDS = 5

const C = {
  app: '#0C0C10',
  stage: '#000000',
  panel: '#16161C',
  border: '#26262E',
  text: '#ECECF1',
  muted: '#9A9AA8',
  faint: '#646472',
  accent: '#F5A524',
  track: '#2E2E38',
}
const FONT_SANS = 'Helvetica'
const FONT_MONO = 'Menlo'

const ICON_PLAY =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000"><path d="M7 4.5v15a1 1 0 0 0 1.5.86l12.5-7.5a1 1 0 0 0 0-1.72L8.5 3.64A1 1 0 0 0 7 4.5Z"/></svg>'
const ICON_PAUSE =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000"><rect x="5" y="4" width="5" height="16" rx="1.5"/><rect x="14" y="4" width="5" height="16" rx="1.5"/></svg>'
const ICON_BACK =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg>'
const ICON_FORWARD =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/></svg>'
const ICON_SOUND =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5Z"/><path d="M16 9a5 5 0 0 1 0 6"/><path d="M19.4 18.4a9 9 0 0 0 0-12.8"/></svg>'
const ICON_MUTED =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4.7a.7.7 0 0 0-1.2-.5L6.4 7.6A1.4 1.4 0 0 1 5.4 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.4a1.4 1.4 0 0 1 1 .4l3.4 3.4a.7.7 0 0 0 1.2-.5Z"/><path d="m22 9-6 6"/><path d="m16 9 6 6"/></svg>'

// ── Player ───────────────────────────────────────────────────────────────────

export interface PlayerState {
  status: 'loading' | 'ready' | 'error'
  playing: boolean
  muted: boolean
  hasAudio: boolean
  currentTime: number
  duration: number
  title: string
  details: string
  error: string | null
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve()
    const timer = setTimeout(done, ms)
    signal.addEventListener('abort', done, { once: true })
    function done() {
      clearTimeout(timer)
      signal.removeEventListener('abort', done)
      resolve()
    }
  })
}

function fitFrame({ width, height }: { width: number; height: number }) {
  const scale = Math.min(1, MAX_FRAME_WIDTH / width, MAX_FRAME_HEIGHT / height)
  return { width: Math.round(width * scale), height: Math.round(height * scale) }
}

function toAudioBuffer(sample: AudioSample): AudioBuffer {
  const buffer = new AudioBuffer({
    numberOfChannels: sample.numberOfChannels,
    length: sample.numberOfFrames,
    sampleRate: sample.sampleRate,
  })
  const channel = new Float32Array(sample.numberOfFrames)
  for (let i = 0; i < sample.numberOfChannels; i++) {
    sample.copyTo(channel, { planeIndex: i, format: 'f32-planar' })
    buffer.copyToChannel(channel, i)
  }
  return buffer
}

export class VideoPlayer {
  state: PlayerState
  private listeners = new Set<() => void>()
  private input: Input | null = null
  private videoSink: VideoSampleSink | null = null
  private audioSink: AudioSampleSink | null = null
  private audio = new AudioContext()
  private gain = this.audio.createGain()
  private audioNodes = new Set<AudioBufferSourceNode>()
  private img: ImgInstance | null = null
  private frameWidth = 0
  private frameHeight = 0
  private bytes: Uint8Array = new Uint8Array(0)
  private hasFrame = false
  // Every present runs after the previous one, because they share `bytes`.
  private presentChain: Promise<void> = Promise.resolve()
  // Aborts the running video and audio loops on pause, seek, or detach.
  private playback: AbortController | null = null
  // Media time `clockTime` is heard at AudioContext time `clockOrigin`.
  private clockOrigin = 0
  private clockTime = 0
  // Latest paused seek. Scrubbing replaces it faster than frames decode.
  private scrubTarget: number | null = null
  private scrubbing = false
  private autoplay = true

  constructor(private source: string) {
    this.gain.connect(this.audio.destination)
    this.state = {
      status: 'loading',
      playing: false,
      muted: false,
      hasAudio: false,
      currentTime: 0,
      duration: 0,
      title: source.split('/').pop() ?? source,
      details: 'Opening…',
      error: null,
    }
    void this.load()
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getSnapshot = () => this.state

  private set(patch: Partial<PlayerState>) {
    this.state = { ...this.state, ...patch }
    for (const listener of this.listeners) listener()
  }

  private async load() {
    const isUrl = /^https?:\/\//.test(this.source)
    const input = new Input({
      source: isUrl ? new UrlSource(this.source) : new FilePathSource(this.source),
      formats: ALL_FORMATS,
    })
    this.input = input
    try {
      const [video, audio] = await Promise.all([
        input.getPrimaryVideoTrack(),
        input.getPrimaryAudioTrack(),
      ])
      if (!video) throw new Error('The file has no video track')
      if (!(await video.canDecode())) throw new Error(`Cannot decode ${video.codec ?? 'this codec'}`)
      const playableAudio = audio && (await audio.canDecode()) ? audio : null
      const [duration, displayWidth, displayHeight, stats] = await Promise.all([
        input.computeDuration(),
        video.getDisplayWidth(),
        video.getDisplayHeight(),
        video.computePacketStats(60),
      ])
      const frame = fitFrame({ width: displayWidth, height: displayHeight })
      this.frameWidth = frame.width
      this.frameHeight = frame.height
      this.bytes = new Uint8Array(frame.width * frame.height * 4)
      this.videoSink = new VideoSampleSink(video)
      this.audioSink = playableAudio && new AudioSampleSink(playableAudio)
      const fps = Math.round(stats.averagePacketRate)
      const sound = playableAudio
        ? `${(playableAudio.codec ?? 'audio').toUpperCase()} ${playableAudio.sampleRate / 1000} kHz`
        : audio
          ? `cannot decode ${audio.codec ?? 'audio'}`
          : 'no audio'
      this.set({
        status: 'ready',
        duration,
        hasAudio: playableAudio !== null,
        details: `${displayWidth}×${displayHeight} · ${(video.codec ?? '?').toUpperCase()} · ${fps} fps · ${sound}`,
      })
      if (this.autoplay) this.play()
      else this.requestFrame(0)
    } catch (error) {
      this.set({ status: 'error', error: error instanceof Error ? error.message : String(error) })
    }
  }

  /** Attach the `<img>` that shows frames. React calls the returned cleanup on unmount. */
  attach = (img: ImgInstance | null) => {
    if (!img) return
    this.img = img
    if (this.hasFrame) this.upload()
    return () => {
      this.autoplay = false
      this.pause()
      if (this.img === img) this.img = null
      // `bun --hot` makes a new player on every save. Release the old file and audio device.
      this.input?.dispose()
      void this.audio.close()
    }
  }

  /** Media time that is reaching the speakers right now. */
  private clock(): number {
    const heard = this.audio.currentTime - this.audio.outputLatency
    return this.clockTime + heard - this.clockOrigin
  }

  private upload() {
    this.img?.setImagePixels(this.frameWidth, this.frameHeight, this.bytes, { format: 'bgra' })
  }

  private present(sample: VideoSample, isStale: () => boolean): Promise<void> {
    const run = this.presentChain.then(async () => {
      if (isStale()) return
      const fits =
        sample.displayWidth === this.frameWidth &&
        sample.displayHeight === this.frameHeight &&
        sample.rotation === 0
      const frame = fits
        ? sample
        : await sample.transform({ width: this.frameWidth, height: this.frameHeight, fit: 'fill' })
      try {
        await frame.copyTo(this.bytes, { format: 'BGRA' })
      } finally {
        if (frame !== sample) frame.close()
      }
      if (isStale()) return
      this.hasFrame = true
      this.upload()
    })
    this.presentChain = run.catch(() => {})
    return run
  }

  togglePlay = () => {
    if (this.state.playing) this.pause()
    else this.play()
  }

  toggleMute = () => {
    const muted = !this.state.muted
    this.gain.gain.value = muted ? 0 : 1
    this.set({ muted })
  }

  play = () => {
    if (!this.videoSink || this.state.playing) return
    const atEnd = this.state.currentTime >= this.state.duration - 0.05
    this.startPlayback(atEnd ? 0 : this.state.currentTime)
  }

  pause = () => {
    this.stopPlayback()
    if (this.state.playing) this.set({ playing: false })
  }

  seek = (time: number) => {
    if (!this.videoSink) return
    const target = Math.min(Math.max(time, 0), this.state.duration)
    if (this.state.playing) this.startPlayback(target)
    else this.requestFrame(target)
  }

  skip = (seconds: number) => this.seek(this.state.currentTime + seconds)

  private stopPlayback() {
    this.playback?.abort()
    this.playback = null
    for (const node of this.audioNodes) node.stop()
    this.audioNodes.clear()
  }

  private startPlayback(from: number) {
    this.stopPlayback()
    const playback = new AbortController()
    this.playback = playback
    this.scrubTarget = null
    this.clockTime = from
    this.clockOrigin = this.audio.currentTime
    this.set({ playing: true, currentTime: from })
    const fail = (error: Error) => {
      if (playback.signal.aborted) return
      this.stopPlayback()
      this.set({ playing: false, status: 'error', error: String(error) })
    }
    void this.runVideo(from, playback.signal).catch(fail)
    void this.runAudio(from, playback.signal).catch(fail)
  }

  private async runVideo(from: number, signal: AbortSignal) {
    let first = true
    for await (const sample of this.videoSink!.samples(from)) {
      try {
        if (signal.aborted) return
        const delay = (sample.timestamp - this.clock()) * 1000
        if (delay < -LATE_FRAME_MS && !first) continue
        if (delay > 0) await sleep(delay, signal)
        if (signal.aborted) return
        await this.present(sample, () => signal.aborted)
        first = false
        if (!signal.aborted) this.set({ currentTime: Math.max(sample.timestamp, 0) })
      } finally {
        sample.close()
      }
    }
    // The audio track can outlast the last frame. Keep the time readout moving.
    while (!signal.aborted && this.clock() < this.state.duration) {
      this.set({ currentTime: Math.max(this.clock(), this.state.currentTime) })
      await sleep(Math.min(250, (this.state.duration - this.clock()) * 1000), signal)
    }
    if (signal.aborted) return
    this.stopPlayback()
    this.set({ playing: false, currentTime: this.state.duration })
  }

  private async runAudio(from: number, signal: AbortSignal) {
    if (!this.audioSink) return
    for await (const sample of this.audioSink.samples(from)) {
      const timestamp = sample.timestamp
      try {
        if (signal.aborted) return
        const node = this.audio.createBufferSource()
        node.buffer = toAudioBuffer(sample)
        node.connect(this.gain)
        const at = this.clockOrigin + timestamp - this.clockTime
        const now = this.audio.currentTime
        // A buffer that is already due starts part way in, so sound never lags the clock.
        if (at >= now) node.start(at)
        else node.start(now, now - at)
        this.audioNodes.add(node)
        node.onended = () => this.audioNodes.delete(node)
      } finally {
        sample.close()
      }
      const ahead = timestamp - this.clock()
      if (ahead > AUDIO_AHEAD_SECONDS) await sleep((ahead - AUDIO_AHEAD_SECONDS) * 1000, signal)
    }
  }

  /** Show the frame at `time` while paused. Only the newest request decodes. */
  private requestFrame(time: number) {
    this.set({ currentTime: time })
    this.scrubTarget = time
    if (this.scrubbing) return
    this.scrubbing = true
    void this.drainFrameRequests().finally(() => {
      this.scrubbing = false
    })
  }

  private async drainFrameRequests() {
    const sink = this.videoSink!
    while (this.scrubTarget !== null) {
      const time = this.scrubTarget
      this.scrubTarget = null
      const sample = await sink.getSample(time)
      if (!sample) continue
      try {
        await this.present(sample, () => this.state.playing || this.scrubTarget !== null)
      } finally {
        sample.close()
      }
    }
  }
}

// ── UI ───────────────────────────────────────────────────────────────────────

function formatTime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds))
  const m = Math.floor(total / 60)
  const s = total % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

function IconButton({
  icon,
  size = 18,
  primary,
  testId,
  onClick,
}: {
  icon: string
  size?: number
  primary?: boolean
  testId: string
  onClick: () => void
}) {
  const box = primary ? 44 : 36
  return (
    <div
      testId={testId}
      onClick={onClick}
      style={{
        width: box,
        height: box,
        flexShrink: 0,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        borderRadius: box / 2,
        backgroundColor: primary ? C.text : C.panel,
        cursor: 'pointer',
        hover: { backgroundColor: primary ? '#FFFFFF' : C.track },
      }}
    >
      <svg
        source={icon}
        style={{ width: size, height: size, color: primary ? C.app : C.text, pointerEvents: 'none' }}
      />
    </div>
  )
}

function SeekBar({ player, state }: { player: VideoPlayer; state: PlayerState }) {
  const { renderer } = useGpuix()
  const bar = useRef<PublicInstance>(null)
  const resumeAfterScrub = useRef(false)
  const progress = state.duration > 0 ? Math.min(state.currentTime / state.duration, 1) : 0

  function timeAt(event: EventPayload): number | null {
    const id = bar.current?.id
    const box = id === undefined ? null : renderer?.getElementBounds?.(id)
    if (!box || box.width <= 0) return null
    const fraction = Math.min(Math.max(((event.x ?? 0) - box.x) / box.width, 0), 1)
    return fraction * state.duration
  }

  return (
    <div
      ref={bar}
      testId="seek-bar"
      onMouseDown={(event) => {
        resumeAfterScrub.current = state.playing
        player.pause()
        const time = timeAt(event)
        if (time !== null) player.seek(time)
      }}
      onMouseMove={(event) => {
        if (event.pressedButton !== 0) return
        const time = timeAt(event)
        if (time !== null) player.seek(time)
      }}
      onMouseUp={() => {
        if (resumeAfterScrub.current) player.play()
        resumeAfterScrub.current = false
      }}
      style={{
        flexGrow: 1,
        height: 24,
        display: 'flex',
        alignItems: 'center',
        cursor: 'pointer',
      }}
    >
      <div
        style={{
          flexGrow: 1,
          height: 6,
          borderRadius: 3,
          backgroundColor: C.track,
          display: 'flex',
          pointerEvents: 'none',
        }}
      >
        <div
          testId="seek-progress"
          style={{
            width: `${progress * 100}%`,
            height: 6,
            borderRadius: 3,
            backgroundColor: C.accent,
          }}
        />
        <div
          style={{
            width: 14,
            height: 14,
            marginLeft: -7,
            marginTop: -4,
            flexShrink: 0,
            borderRadius: 7,
            backgroundColor: C.text,
          }}
        />
      </div>
    </div>
  )
}

export function VideoPlayerApp({ player }: { player: VideoPlayer }) {
  const state = useSyncExternalStore(player.subscribe, player.getSnapshot)
  const ready = state.status === 'ready'

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        gap: 16,
        padding: 20,
        backgroundColor: C.app,
        fontFamily: FONT_SANS,
      }}
    >
      <div
        testId="stage"
        onClick={player.togglePlay}
        style={{
          flexGrow: 1,
          minHeight: 0,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          borderRadius: 14,
          borderWidth: 1,
          borderColor: C.border,
          backgroundColor: C.stage,
          overflow: 'hidden',
          position: 'relative',
          cursor: ready ? 'pointer' : 'default',
        }}
      >
        <img
          ref={player.attach}
          testId="video-frame"
          objectFit="contain"
          style={{ width: '100%', height: '100%' }}
        />
        {state.status !== 'ready' || !state.playing ? (
          <div
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              pointerEvents: 'none',
            }}
          >
            {state.status === 'ready' ? (
              <div
                style={{
                  width: 76,
                  height: 76,
                  borderRadius: 38,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  backgroundColor: '#000000A0',
                  borderWidth: 1,
                  borderColor: '#FFFFFF30',
                }}
              >
                <svg source={ICON_PLAY} style={{ width: 30, height: 30, marginLeft: 4, color: C.text }} />
              </div>
            ) : (
              <text
                testId="stage-status"
                style={{ color: state.error ? '#FF6B6B' : C.muted, fontSize: 14 }}
              >
                {state.error ?? 'Loading video…'}
              </text>
            )}
          </div>
        ) : null}
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <IconButton testId="skip-back" icon={ICON_BACK} onClick={() => player.skip(-SEEK_STEP_SECONDS)} />
        <IconButton
          testId="play-pause"
          primary
          icon={state.playing ? ICON_PAUSE : ICON_PLAY}
          onClick={player.togglePlay}
        />
        <IconButton testId="skip-forward" icon={ICON_FORWARD} onClick={() => player.skip(SEEK_STEP_SECONDS)} />
        <text
          testId="time"
          style={{ width: 96, color: C.text, fontSize: 13, fontFamily: FONT_MONO, textAlign: 'center' }}
        >
          {`${formatTime(state.currentTime)} / ${formatTime(state.duration)}`}
        </text>
        <SeekBar player={player} state={state} />
        {state.hasAudio ? (
          <IconButton
            testId="mute"
            icon={state.muted ? ICON_MUTED : ICON_SOUND}
            onClick={player.toggleMute}
          />
        ) : null}
      </div>

      <div style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
        <text testId="title" style={{ color: C.text, fontSize: 15, fontWeight: 600 }}>
          {state.title}
        </text>
        <text testId="details" style={{ color: C.muted, fontSize: 12 }}>
          {state.details}
        </text>
        <div style={{ flexGrow: 1 }} />
        <text style={{ color: C.faint, fontSize: 11 }}>Space play · ← → seek · M mute · Mediabunny + FFmpeg</text>
      </div>
    </div>
  )
}

if (import.meta.main) {
  const player = new VideoPlayer(process.argv[2] ?? DEFAULT_SOURCE)
  render(<VideoPlayerApp player={player} />, {
    title: 'GPUIX · Video Player',
    width: 1040,
    height: 720,
    minWidth: 560,
    minHeight: 400,
    focus: process.env.GPUIX_BACKGROUND !== '1',
    onKeyDown(event) {
      if (event.key === 'space') player.togglePlay()
      if (event.key === 'left') player.skip(-SEEK_STEP_SECONDS)
      if (event.key === 'right') player.skip(SEEK_STEP_SECONDS)
      if (event.key === 'm') player.toggleMute()
    },
  })
}
