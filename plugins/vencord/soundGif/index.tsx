/*
 * Vencord, a Discord client mod
 * Copyright (c) 2026 Superwheat
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

import "./styles.css";

import { definePluginSettings } from "@api/Settings";
import definePlugin, { makeRange, OptionType } from "@utils/types";
import { Message, MessageAttachment } from "@vencord/discord-types";
import { ReactDOM, useEffect, useRef, useState, useStateFromStores, WindowStore } from "@webpack/common";

const settings = definePluginSettings({
    defaultVolume: {
        type: OptionType.SLIDER,
        description: "Default SoundGIF playback volume",
        markers: makeRange(0, 100, 5),
        default: 30,
        stickToMarkers: false
    },
    normalizeVolume: {
        type: OptionType.BOOLEAN,
        description: "Reduce loud SoundGIFs to a safer level",
        default: true
    },
    autoplay: {
        type: OptionType.BOOLEAN,
        description: "Play SoundGIF audio automatically while it is visible",
        default: true
    }
});

const MAX_ATTACHMENT_BYTES = 50 * 1024 * 1024;
const MAX_AUDIO_BYTES = 16 * 1024 * 1024;
const MAX_AUDIO_DURATION_SECONDS = 5 * 60;
const MAX_CACHE_ENTRIES = 8;
const ALLOWED_AUDIO_TYPES = new Set([
    "audio/aac",
    "audio/flac",
    "audio/mp4",
    "audio/mpeg",
    "audio/ogg",
    "audio/opus",
    "audio/webm"
]);

interface SoundPayload {
    audio: Uint8Array;
    gif: Uint8Array;
    gifDurationMs: number;
    loop: boolean;
    mime: string;
    startMs: number;
}

interface PlayerGraph {
    audio: HTMLAudioElement;
    context: AudioContext;
    gain: GainNode;
    limiter: DynamicsCompressorNode;
    normalization: number;
    source: MediaElementAudioSourceNode;
    url: string;
}

type PlaybackState = "ready" | "blocked";

const payloadCache = new Map<string, Promise<SoundPayload | null>>();
const individuallyMuted = new Map<string, boolean>();
const activePlayers = new Set<PlayerGraph>();
let sharedAudioContext: AudioContext | undefined;

function getAudioContext() {
    return sharedAudioContext ??= new AudioContext();
}

function isGifAttachment(attachment: MessageAttachment) {
    return attachment.content_type === "image/gif"
        || attachment.filename.toLowerCase().endsWith(".gif");
}

function loadAttachment(attachment: MessageAttachment) {
    let pending = payloadCache.get(attachment.url);
    if (pending) {
        payloadCache.delete(attachment.url);
        payloadCache.set(attachment.url, pending);
        return pending;
    }

    pending = (async () => {
        // Avoid allocating unreasonable amounts of memory for a malicious attachment.
        if (attachment.size <= 0 || attachment.size > MAX_ATTACHMENT_BYTES) return null;

        try {
            const url = new URL(attachment.url);
            if (url.protocol !== "https:") return null;

            const response = await fetch(url, {
                cache: "force-cache",
                credentials: "omit",
                redirect: "error",
                referrerPolicy: "no-referrer"
            });
            if (!response.ok) return null;
            const declaredLength = Number(response.headers.get("content-length"));
            if (Number.isFinite(declaredLength) && declaredLength > MAX_ATTACHMENT_BYTES) return null;

            const bytes = await readLimitedResponse(response, MAX_ATTACHMENT_BYTES);
            return bytes && parseSoundGif(bytes);
        } catch {
            return null;
        }
    })();

    if (payloadCache.size >= MAX_CACHE_ENTRIES) {
        const oldest = payloadCache.keys().next().value;
        if (oldest) payloadCache.delete(oldest);
    }
    payloadCache.set(attachment.url, pending);
    return pending;
}

async function readLimitedResponse(response: Response, limit: number) {
    if (!response.body) {
        const buffer = await response.arrayBuffer();
        return buffer.byteLength <= limit ? new Uint8Array(buffer) : null;
    }

    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let total = 0;
    try {
        while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            total += value.byteLength;
            if (total > limit) {
                await reader.cancel();
                return null;
            }
            chunks.push(value);
        }
    } finally {
        reader.releaseLock();
    }

    const bytes = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
        bytes.set(chunk, offset);
        offset += chunk.byteLength;
    }
    return bytes;
}

function parseSoundGif(bytes: Uint8Array): SoundPayload | null {
    if (bytes.length < 14 || ascii(bytes, 0, 3) !== "GIF") return null;

    let cursor = 13;
    let pendingFrameDelayMs = 100;
    const frameDelaysMs: number[] = [];
    if (bytes[10] & 0x80) cursor += 3 << ((bytes[10] & 7) + 1);

    while (cursor < bytes.length) {
        const marker = bytes[cursor];
        if (marker === 0x3b) return null;

        if (marker === 0x2c) {
            requireBytes(bytes, cursor, 10);
            frameDelaysMs.push(pendingFrameDelayMs);
            pendingFrameDelayMs = 100;
            const packed = bytes[cursor + 9];
            cursor += 10;
            if (packed & 0x80) cursor += 3 << ((packed & 7) + 1);
            requireBytes(bytes, cursor, 1);
            cursor = readSubBlocks(bytes, cursor + 1, false).cursor;
            continue;
        }

        if (marker !== 0x21) return null;
        requireBytes(bytes, cursor, 2);
        const label = bytes[cursor + 1];
        cursor += 2;
        let isSoundGif = false;

        if (label === 0xf9) {
            requireBytes(bytes, cursor, 6);
            if (bytes[cursor] === 4) {
                const delayHundredths = bytes[cursor + 2] | (bytes[cursor + 3] << 8);
                pendingFrameDelayMs = Math.max(20, delayHundredths * 10);
            }
        }

        if (label === 0xff) {
            requireBytes(bytes, cursor, 1);
            const length = bytes[cursor];
            requireBytes(bytes, cursor + 1, length);
            isSoundGif = length === 11 && ascii(bytes, cursor + 1, 11) === "SNDGIF01001";
            cursor += length + 1;
        }

        const blocks = readSubBlocks(bytes, cursor, isSoundGif);
        cursor = blocks.cursor;
        if (isSoundGif && blocks.data) {
            const payload = decodePayload(blocks.data);
            if (!payload) return null;
            return {
                ...payload,
                gif: bytes.slice(),
                gifDurationMs: Math.max(100, frameDelaysMs.reduce((total, delay) => total + delay, 0))
            };
        }
    }

    return null;
}

function readSubBlocks(bytes: Uint8Array, start: number, collect: boolean) {
    const chunks: Uint8Array[] = [];
    let cursor = start;
    let total = 0;

    while (true) {
        requireBytes(bytes, cursor, 1);
        const size = bytes[cursor++];
        if (size === 0) break;
        requireBytes(bytes, cursor, size);
        if (collect) {
            chunks.push(bytes.subarray(cursor, cursor + size));
            total += size;
        }
        cursor += size;
    }

    if (!collect) return { cursor, data: undefined as Uint8Array | undefined };
    const data = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
        data.set(chunk, offset);
        offset += chunk.length;
    }
    return { cursor, data };
}

function decodePayload(data: Uint8Array): Omit<SoundPayload, "gif" | "gifDurationMs"> | null {
    requireBytes(data, 0, 26);
    if (ascii(data, 0, 4) !== "SGA1" || data[4] !== 1) return null;

    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const audioLengthBig = view.getBigUint64(10, true);
    if (audioLengthBig > BigInt(Number.MAX_SAFE_INTEGER)) return null;

    const audioLength = Number(audioLengthBig);
    const mimeLength = view.getUint16(22, true);
    const nameLength = view.getUint16(24, true);
    if (audioLength <= 0 || audioLength > MAX_AUDIO_BYTES || mimeLength > 64 || nameLength > 512) return null;
    const metadataEnd = 26 + mimeLength + nameLength;
    if (metadataEnd + audioLength !== data.length) return null;

    const audio = data.slice(metadataEnd);
    if (crc32(audio) !== view.getUint32(18, true)) return null;

    const mime = new TextDecoder("utf-8", { fatal: true })
        .decode(data.subarray(26, 26 + mimeLength))
        .toLowerCase();
    if (!ALLOWED_AUDIO_TYPES.has(mime)) return null;

    return {
        audio,
        loop: Boolean(data[5] & 1),
        mime,
        startMs: view.getUint32(6, true)
    };
}

function requireBytes(bytes: Uint8Array, start: number, length: number) {
    if (start < 0 || start + length > bytes.length) throw new Error("Truncated SoundGIF");
}

function ascii(bytes: Uint8Array, start: number, length: number) {
    let value = "";
    for (let index = start; index < start + length; index++) value += String.fromCharCode(bytes[index]);
    return value;
}

function crc32(bytes: Uint8Array) {
    let crc = 0xffffffff;
    for (const byte of bytes) {
        crc = (crc ^ byte) >>> 0;
        for (let bit = 0; bit < 8; bit++)
            crc = ((crc >>> 1) ^ (0xedb88320 & -(crc & 1))) >>> 0;
    }
    return (~crc) >>> 0;
}

async function calculateNormalization(audio: Uint8Array) {
    try {
        const decoded = await getAudioContext().decodeAudioData(audio.slice().buffer as ArrayBuffer);
        const stride = Math.max(1, Math.floor(decoded.length / 1_000_000));
        let squares = 0;
        let peak = 0;
        let samples = 0;

        for (let channel = 0; channel < decoded.numberOfChannels; channel++) {
            const values = decoded.getChannelData(channel);
            for (let index = 0; index < values.length; index += stride) {
                const absolute = Math.abs(values[index]);
                peak = Math.max(peak, absolute);
                squares += values[index] * values[index];
                samples++;
            }
        }

        if (samples === 0) return 1;
        const rms = Math.sqrt(squares / samples);
        const rmsGain = 0.1 / Math.max(rms, 0.000001); // -20 dBFS target
        const peakGain = 0.5 / Math.max(peak, 0.000001); // -6 dBFS ceiling
        return Math.min(1, rmsGain, peakGain);
    } catch {
        // The real-time limiter still protects playback if WebAudio cannot decode this codec.
        return 1;
    }
}

async function createPlayer(payload: SoundPayload) {
    const context = getAudioContext();
    const audioBuffer = payload.audio.slice().buffer as ArrayBuffer;
    const url = URL.createObjectURL(new Blob([audioBuffer], { type: payload.mime }));
    const audio = new Audio(url);
    // GIF and audio looping are driven by one shared timeline below.
    audio.loop = false;
    audio.preload = "auto";

    try {
        await new Promise<void>((resolve, reject) => {
            const timeout = setTimeout(() => reject(new Error("Timed out reading audio metadata")), 10_000);
            const done = (callback: () => void) => {
                clearTimeout(timeout);
                audio.removeEventListener("loadedmetadata", loaded);
                audio.removeEventListener("error", failed);
                callback();
            };
            const loaded = () => done(resolve);
            const failed = () => done(() => reject(new Error("Unsupported audio stream")));
            audio.addEventListener("loadedmetadata", loaded);
            audio.addEventListener("error", failed);
            audio.load();
        });
        if (!Number.isFinite(audio.duration) || audio.duration <= 0 || audio.duration > MAX_AUDIO_DURATION_SECONDS)
            throw new Error("Unsafe audio duration");
        if (payload.startMs > audio.duration * 1000)
            throw new Error("Invalid SoundGIF start offset");
    } catch (error) {
        URL.revokeObjectURL(url);
        throw error;
    }

    const source = context.createMediaElementSource(audio);
    const gain = context.createGain();
    const limiter = context.createDynamicsCompressor();
    limiter.threshold.value = -12;
    limiter.knee.value = 3;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.003;
    limiter.release.value = 0.2;
    source.connect(gain).connect(limiter).connect(context.destination);

    const player: PlayerGraph = {
        audio,
        context,
        gain,
        limiter,
        normalization: await calculateNormalization(payload.audio),
        source,
        url
    };
    activePlayers.add(player);
    return player;
}

function destroyPlayer(player: PlayerGraph) {
    if (!activePlayers.delete(player)) return;
    player.audio.pause();
    player.audio.removeAttribute("src");
    player.audio.load();
    player.source.disconnect();
    player.gain.disconnect();
    player.limiter.disconnect();
    URL.revokeObjectURL(player.url);
}

function applyVolume(player: PlayerGraph, volume: number, normalize: boolean, muted: boolean) {
    const safeVolume = Math.max(0, Math.min(1, volume / 100));
    const normalization = normalize ? player.normalization : 1;
    player.gain.gain.setTargetAtTime(muted ? 0 : safeVolume * normalization, player.context.currentTime, 0.01);
}

async function play(player: PlayerGraph, setState: (state: PlaybackState) => void) {
    try {
        await player.context.resume();
        await player.audio.play();
        setState("ready");
    } catch {
        setState("blocked");
    }
}

function SpeakerIcon({ muted }: { muted: boolean; }) {
    return muted ? (
        <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M2 6h3l3-3v10l-3-3H2V6Zm8.5.2 3.3 3.3m0-3.3-3.3 3.3" />
        </svg>
    ) : (
        <svg viewBox="0 0 16 16" aria-hidden="true">
            <path d="M2 6h3l3-3v10l-3-3H2V6Zm8-1.5a5 5 0 0 1 0 7M9.5 6a2.8 2.8 0 0 1 0 4" />
        </svg>
    );
}

function findAttachmentImage(anchor: HTMLElement, attachment: MessageAttachment) {
    const message = anchor.closest<HTMLElement>("[id^='chat-messages-']");
    if (!message) return null;

    for (const image of message.querySelectorAll<HTMLImageElement>("img")) {
        try {
            const path = new URL(image.currentSrc || image.src).pathname;
            if (path.split("/").includes(attachment.id)) return image;
        } catch { }
    }
    return null;
}

function imageAppearsAnimated(image: HTMLImageElement | null) {
    if (!image) return false;
    try {
        const url = new URL(image.currentSrc || image.src);
        if (url.searchParams.get("animated") === "false") return false;
        return true;
    } catch {
        return false;
    }
}

function SoundGifControl({ attachment }: { attachment: MessageAttachment; }) {
    const { autoplay, defaultVolume, normalizeVolume } = settings.use(["autoplay", "defaultVolume", "normalizeVolume"]);
    const windowFocused = useStateFromStores([WindowStore], () => WindowStore.isFocused());
    const [detected, setDetected] = useState(false);
    const [muted, setMuted] = useState(individuallyMuted.get(attachment.id) ?? false);
    const [playbackState, setPlaybackState] = useState<PlaybackState>("ready");
    const [overlayHost, setOverlayHost] = useState<HTMLDivElement | null>(null);
    const anchorRef = useRef<HTMLSpanElement>(null);
    const sourceImageRef = useRef<HTMLImageElement | null>(null);
    const playbackImageRef = useRef<HTMLImageElement | null>(null);
    const playerRef = useRef<PlayerGraph | null>(null);
    const gifUrlRef = useRef<string | null>(null);
    const gifDurationMsRef = useRef(1000);
    const loopAudioRef = useRef(true);
    const startMsRef = useRef(0);
    const visibleRef = useRef(false);
    const timelineRunningRef = useRef(false);
    const cycleStartedAtRef = useRef(0);

    useEffect(() => {
        let disposed = false;

        loadAttachment(attachment).then(async payload => {
            if (!payload || disposed) return;
            let player: PlayerGraph;
            try {
                player = await createPlayer(payload);
            } catch {
                return;
            }
            if (disposed) {
                destroyPlayer(player);
                return;
            }

            playerRef.current = player;
            const gifBuffer = payload.gif.slice().buffer as ArrayBuffer;
            gifUrlRef.current = URL.createObjectURL(new Blob([gifBuffer], { type: "image/gif" }));
            gifDurationMsRef.current = payload.gifDurationMs;
            loopAudioRef.current = payload.loop;
            startMsRef.current = payload.startMs;
            applyVolume(player, defaultVolume, normalizeVolume, muted);
            setDetected(true);
        });

        return () => {
            disposed = true;
            if (playerRef.current) destroyPlayer(playerRef.current);
            if (gifUrlRef.current) URL.revokeObjectURL(gifUrlRef.current);
            playerRef.current = null;
            gifUrlRef.current = null;
        };
    }, [attachment.url]);

    useEffect(() => {
        const player = playerRef.current;
        if (player) applyVolume(player, defaultVolume, normalizeVolume, muted);
    }, [defaultVolume, normalizeVolume, muted, detected]);

    useEffect(() => {
        const anchor = anchorRef.current;
        if (!anchor || !detected) return;

        let host: HTMLDivElement | null = null;
        let parent: HTMLElement | null = null;
        let playbackImage: HTMLImageElement | null = null;

        const attachOverlay = () => {
            if (host?.isConnected && sourceImageRef.current?.isConnected) return;
            host?.remove();
            playbackImage?.remove();
            parent?.classList.remove("vc-soundgif-overlay-parent");

            const image = findAttachmentImage(anchor, attachment);
            const imageParent = image?.parentElement;
            const gifUrl = gifUrlRef.current;
            if (!image || !imageParent || !gifUrl) return;

            sourceImageRef.current = image;
            parent = imageParent;
            parent.classList.add("vc-soundgif-overlay-parent");

            playbackImage = image.cloneNode(false) as HTMLImageElement;
            playbackImage.classList.add("vc-soundgif-playback-image");
            playbackImage.removeAttribute("srcset");
            playbackImage.removeAttribute("sizes");
            playbackImage.src = gifUrl;
            playbackImage.style.display = "none";
            image.insertAdjacentElement("afterend", playbackImage);
            playbackImageRef.current = playbackImage;

            host = document.createElement("div");
            host.className = "vc-soundgif-overlay-host";
            parent.appendChild(host);
            setOverlayHost(host);
        };

        attachOverlay();
        const message = anchor.closest<HTMLElement>("[id^='chat-messages-']");
        const observer = new MutationObserver(attachOverlay);
        if (message) observer.observe(message, { childList: true, subtree: true });

        return () => {
            observer.disconnect();
            host?.remove();
            playbackImage?.remove();
            parent?.classList.remove("vc-soundgif-overlay-parent");
            sourceImageRef.current = null;
            playbackImageRef.current = null;
            setOverlayHost(null);
        };
    }, [attachment.id, detected]);

    useEffect(() => {
        const sourceImage = sourceImageRef.current;
        const playbackImage = playbackImageRef.current;
        const player = playerRef.current;
        const gifUrl = gifUrlRef.current;
        if (!sourceImage || !playbackImage || !player || !gifUrl || !detected || !overlayHost) return;

        let startTimer: ReturnType<typeof setTimeout> | undefined;
        let cycleTimer: ReturnType<typeof setTimeout> | undefined;
        let cycleIndex = 0;

        const clearTimers = () => {
            clearTimeout(startTimer);
            clearTimeout(cycleTimer);
            startTimer = undefined;
            cycleTimer = undefined;
        };

        const restartCycle = () => {
            if (!timelineRunningRef.current) return;
            clearTimers();
            cycleStartedAtRef.current = performance.now();

            playbackImage.style.display = "";
            playbackImage.src = "";
            void playbackImage.offsetWidth;
            playbackImage.src = gifUrl;

            player.audio.pause();
            player.audio.currentTime = 0;
            const shouldPlayAudio = loopAudioRef.current || cycleIndex === 0;
            if (shouldPlayAudio) {
                startTimer = setTimeout(() => {
                    startTimer = undefined;
                    if (timelineRunningRef.current) void play(player, setPlaybackState);
                }, startMsRef.current);
            }

            cycleIndex++;
            cycleTimer = setTimeout(restartCycle, gifDurationMsRef.current);
        };

        const stopTimeline = () => {
            if (!timelineRunningRef.current) return;
            timelineRunningRef.current = false;
            clearTimers();
            player.audio.pause();
            playbackImage.style.display = "none";
        };

        const syncPlayback = () => {
            const shouldPlay = autoplay
                && visibleRef.current
                && document.visibilityState === "visible"
                && windowFocused
                && imageAppearsAnimated(sourceImage);

            if (!shouldPlay) {
                stopTimeline();
                return;
            }

            if (!timelineRunningRef.current) {
                timelineRunningRef.current = true;
                cycleIndex = 0;
                restartCycle();
                return;
            }

            const expectedSeconds = (performance.now() - cycleStartedAtRef.current - startMsRef.current) / 1000;
            if (
                expectedSeconds >= 0
                && expectedSeconds < player.audio.duration
                && !player.audio.paused
                && Math.abs(player.audio.currentTime - expectedSeconds) > 0.08
            ) player.audio.currentTime = expectedSeconds;
        };

        const observer = new IntersectionObserver(entries => {
            visibleRef.current = entries[0]?.isIntersecting ?? false;
            syncPlayback();
        }, { threshold: 0.1 });

        const sourceObserver = new MutationObserver(syncPlayback);
        const onPageStateChanged = () => syncPlayback();
        const monitor = setInterval(syncPlayback, 250);

        observer.observe(sourceImage);
        sourceObserver.observe(sourceImage, { attributeFilter: ["src", "srcset"], attributes: true });
        document.addEventListener("visibilitychange", onPageStateChanged);
        window.addEventListener("resize", onPageStateChanged);
        syncPlayback();

        return () => {
            clearInterval(monitor);
            stopTimeline();
            observer.disconnect();
            sourceObserver.disconnect();
            document.removeEventListener("visibilitychange", onPageStateChanged);
            window.removeEventListener("resize", onPageStateChanged);
        };
    }, [attachment.id, autoplay, detected, overlayHost, windowFocused]);

    if (!detected) return <span className="vc-soundgif-anchor" ref={anchorRef} />;

    const onClick = () => {
        const player = playerRef.current;
        if (!player) return;

        if (playbackState === "blocked") {
            setMuted(false);
            individuallyMuted.set(attachment.id, false);
            applyVolume(player, defaultVolume, normalizeVolume, false);
            if (timelineRunningRef.current) {
                const expectedSeconds = Math.max(
                    0,
                    (performance.now() - cycleStartedAtRef.current - startMsRef.current) / 1000
                );
                player.audio.currentTime = Math.min(expectedSeconds, Math.max(0, player.audio.duration - 0.01));
                void play(player, setPlaybackState);
            }
            return;
        }

        const nextMuted = !muted;
        setMuted(nextMuted);
        individuallyMuted.set(attachment.id, nextMuted);
        applyVolume(player, defaultVolume, normalizeVolume, nextMuted);
    };

    const label = playbackState === "blocked" ? "Play sound" : muted ? "Muted" : "Sound on";
    const control = (
        <button
            className={`vc-soundgif-pill ${muted ? "vc-soundgif-muted" : ""}`}
            onClick={event => {
                event.preventDefault();
                event.stopPropagation();
                onClick();
            }}
            onMouseDown={event => event.stopPropagation()}
            title={`${label} · default volume ${defaultVolume}%`}
            type="button"
        >
            <SpeakerIcon muted={muted} />
            <span>{label}</span>
        </button>
    );

    return (
        <>
            <span className="vc-soundgif-anchor" ref={anchorRef} />
            {overlayHost && ReactDOM.createPortal(control, overlayHost)}
        </>
    );
}

function SoundGifAccessories({ message }: { message: Message; }) {
    const attachments = message.attachments.filter(isGifAttachment);
    if (!attachments.length) return null;

    return <>{attachments.map(attachment => <SoundGifControl attachment={attachment} key={attachment.id} />)}</>;
}

export default definePlugin({
    name: "SoundGIF",
    description: "Plays normalized audio embedded in SoundGIF attachments",
    tags: ["Chat", "Media"],
    authors: [{ name: "Superwheat", id: 0n }],
    settings,

    renderMessageAccessory: props => <SoundGifAccessories message={props.message} />,

    stop() {
        for (const player of activePlayers) destroyPlayer(player);
        payloadCache.clear();
        individuallyMuted.clear();
        void sharedAudioContext?.close();
        sharedAudioContext = undefined;
    }
});
