import { useEffect, useRef, useState } from "react";

type Analysis = {
  id: string;
  audio_id: string;
  excerpt: { start: number; duration: number };
  processing: { separation?: { guitar_audio_id?: string } };
  measurements: { rms_dbfs: number };
};
export function Audition({
  reference,
  recording,
  fetchAudio,
}: {
  reference: Analysis;
  recording: Analysis;
  fetchAudio: (id: string) => Promise<Blob>;
}) {
  const [urls, setUrls] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [matched, setMatched] = useState(true);
  const players = useRef<(HTMLAudioElement | null)[]>([]);
  const audioContext = useRef<AudioContext | null>(null);
  const gains = useRef<GainNode[]>([]);
  useEffect(() => {
    let alive = true;
    const allocated: string[] = [];
    Promise.all(
      [reference, recording].map(async (a) => {
        const id = a.processing.separation?.guitar_audio_id ?? a.audio_id;
        const url = URL.createObjectURL(await fetchAudio(id));
        allocated.push(url);
        return url;
      }),
    )
      .then((u) => {
        if (alive) setUrls(u);
      })
      .catch((e) => alive && setError(e.message));
    return () => {
      alive = false;
      allocated.forEach((u) => URL.revokeObjectURL(u));
    };
  }, [reference.id, recording.id]);
  useEffect(
    () => () => {
      audioContext.current?.close();
    },
    [],
  );
  const reports = [reference, recording];
  const target = Math.min(...reports.map((r) => r.measurements.rms_dbfs));
  const levels = reports.map((r) =>
    matched ? Math.pow(10, (target - r.measurements.rms_dbfs) / 20) : 1,
  );
  useEffect(() => {
    gains.current.forEach((gain, i) => {
      gain.gain.value = levels[i];
    });
  }, [matched, reference.id, recording.id]);
  async function play(index: number) {
    players.current.forEach((p, i) => {
      if (i !== index) p?.pause();
    });
    if (!audioContext.current) {
      audioContext.current = new AudioContext();
      players.current.forEach((p) => {
        if (p) {
          const source = audioContext.current!.createMediaElementSource(p);
          const gain = audioContext.current!.createGain();
          source.connect(gain).connect(audioContext.current!.destination);
          gains.current.push(gain);
        }
      });
    }
    gains.current.forEach((g, i) => {
      g.gain.value = levels[i];
    });
    await audioContext.current.resume();
  }
  return (
    <div className="audition">
      <h3>A/B listening</h3>
      <label className="check">
        <input
          type="checkbox"
          checked={matched}
          onChange={(e) => setMatched(e.target.checked)}
        />{" "}
        Match excerpt RMS levels (attenuation only)
      </label>
      {error && <p role="status">{error}</p>}
      {reports.map((a, i) => {
        const start = a.processing.separation?.guitar_audio_id
          ? 0
          : a.excerpt.start;
        return (
          <div key={i}>
            <strong>{i === 0 ? "A · Reference" : "B · Your recording"}</strong>
            <small>
              {matched
                ? (20 * Math.log10(levels[i])).toFixed(1) + " dB playback gain"
                : "Original level"}
            </small>
            <audio
              aria-label={
                i === 0 ? "Reference A playback" : "Recording B playback"
              }
              ref={(el) => {
                players.current[i] = el;
              }}
              controls
              src={urls[i]}
              onLoadedMetadata={(e) => {
                e.currentTarget.currentTime = start;
              }}
              onPlay={() => play(i).catch((e) => setError(e.message))}
              onTimeUpdate={(e) => {
                if (e.currentTarget.currentTime > start + a.excerpt.duration) {
                  e.currentTarget.pause();
                  e.currentTarget.currentTime = start;
                }
              }}
            />
          </div>
        );
      })}
      <p className="caption">
        RMS matching reduces loudness bias. Different notes and playing still
        affect comparison.
      </p>
    </div>
  );
}
