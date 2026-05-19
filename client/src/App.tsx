import { useEffect, useRef, useState, useCallback } from 'react';
import { mat4 } from 'gl-matrix';
import { WebGLRenderer } from './engine/renderer';
import { WtClient } from './engine/wt-client';
import { GameState } from './game/state';
import { Snapshot, EntityType } from './types';

const SERVER_URL = 'https://localhost:4433';

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';

export default function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const gameStateRef = useRef(new GameState());
  const clientRef = useRef<WtClient | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [entityCount, setEntityCount] = useState(0);
  const [playerBaseHealth, setPlayerBaseHealth] = useState({ current: 0, max: 0 });
  const [enemyBaseHealth, setEnemyBaseHealth] = useState({ current: 0, max: 0 });
  const [tick, setTick] = useState(0);
  const animFrameRef = useRef<number>(0);
  const lastSnapshotTimeRef = useRef(0);
  const snapshotIntervalRef = useRef(33);

  const handleSnapshot = useCallback((snapshot: Snapshot) => {
    gameStateRef.current.applySnapshot(snapshot);
    setEntityCount(snapshot.entities.length);
    setTick(snapshot.tick);

    const pb = gameStateRef.current.getBaseHealth(0);
    if (pb) setPlayerBaseHealth(pb);

    const eb = gameStateRef.current.getBaseHealth(1);
    if (eb) setEnemyBaseHealth(eb);

    lastSnapshotTimeRef.current = performance.now();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    rendererRef.current = new WebGLRenderer(canvas);

    const client = new WtClient(SERVER_URL);
    client.setOnSnapshot(handleSnapshot);
    clientRef.current = client;

    let lastTime = performance.now();

    const loop = (time: number) => {
      const dt = (time - lastTime) / 1000;
      lastTime = time;

      const elapsedSinceSnapshot = time - lastSnapshotTimeRef.current;
      const factor = Math.min(elapsedSinceSnapshot / snapshotIntervalRef.current, 1.0);
      gameStateRef.current.interpolate(factor);

      const zoom = 2.0;
      const viewportWidth = canvas.width * zoom;
      const viewportHeight = canvas.height * zoom;
      const aspect = canvas.width / canvas.height;

      const projectionMatrix = mat4.create();
      mat4.perspective(projectionMatrix, Math.PI / 4, aspect, 10, 10000);

      const viewMatrix = mat4.create();
      const cameraZ = Math.max(viewportWidth, viewportHeight);
      mat4.lookAt(viewMatrix, [0, 0, cameraZ], [0, 0, 0], [0, 1, 0]);

      const { positions, colors, sizes, types, count } = gameStateRef.current.getInterpolatedPositions();

      rendererRef.current?.render(
        positions,
        colors,
        sizes,
        types,
        count,
        viewMatrix,
        projectionMatrix,
        time / 1000,
      );

      animFrameRef.current = requestAnimationFrame(loop);
    };

    animFrameRef.current = requestAnimationFrame(loop);

    const handleResize = () => {
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;
    };

    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animFrameRef.current);
      window.removeEventListener('resize', handleResize);
      client.disconnect();
    };
  }, [handleSnapshot]);

  const connect = async () => {
    setConnectionState('connecting');
    try {
      await clientRef.current?.connect();
      setConnectionState('connected');
    } catch {
      setConnectionState('error');
    }
  };

  return (
    <div className="relative w-full h-screen bg-slate-950 overflow-hidden font-sans text-white">
      <canvas ref={canvasRef} className="absolute inset-0 w-full h-full" />

      {connectionState === 'disconnected' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/80 backdrop-blur-sm">
          <h1 className="text-8xl font-black italic tracking-tighter uppercase mb-2 text-transparent bg-clip-text bg-gradient-to-b from-white to-white/20">
            STRIKE 2.5D
          </h1>
          <p className="text-white/40 uppercase tracking-[0.5em] text-sm mb-12">AI Arena Spectator</p>
          <button
            onClick={connect}
            className="group relative flex items-center gap-4 bg-white text-black px-12 py-5 rounded-full font-bold text-xl hover:scale-105 transition-all active:scale-95"
          >
            CONNECT TO SERVER
            <div className="absolute -inset-1 bg-white/20 rounded-full blur opacity-0 group-hover:opacity-100 transition-opacity" />
          </button>
        </div>
      )}

      {connectionState === 'connecting' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/80 backdrop-blur-sm">
          <h1 className="text-6xl font-black italic tracking-tighter uppercase mb-4 text-white/60">
            CONNECTING...
          </h1>
          <p className="text-white/30 uppercase tracking-widest text-sm">Establishing WebTransport link</p>
        </div>
      )}

      {connectionState === 'error' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-red-950/80 backdrop-blur-sm">
          <h1 className="text-6xl font-black italic tracking-tighter uppercase mb-4 text-red-400">
            CONNECTION FAILED
          </h1>
          <p className="text-red-300/60 mb-8">Is the server running on port 4433?</p>
          <button
            onClick={connect}
            className="flex items-center gap-4 bg-white text-black px-12 py-5 rounded-full font-bold text-xl hover:scale-105 transition-all active:scale-95"
          >
            RETRY
          </button>
        </div>
      )}

      {connectionState === 'connected' && (
        <>
          <div className="absolute top-6 left-6 flex flex-col gap-4 pointer-events-none">
            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-2xl border border-white/10">
              <div className="w-3 h-3 rounded-full bg-emerald-400 animate-pulse" />
              <span className="font-mono font-bold text-sm text-emerald-400">LIVE</span>
            </div>

            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-2xl border border-white/10">
              <div className="w-48 h-3 bg-white/10 rounded-full overflow-hidden">
                <div
                  className="h-full bg-blue-400 transition-all duration-300"
                  style={{ width: `${playerBaseHealth.max > 0 ? (playerBaseHealth.current / playerBaseHealth.max) * 100 : 0}%` }}
                />
              </div>
              <span className="font-mono font-bold text-lg">{Math.ceil(playerBaseHealth.current)}</span>
            </div>
          </div>

          <div className="absolute top-6 right-6 flex flex-col items-end gap-4 pointer-events-none">
            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-2xl border border-white/10">
              <span className="font-mono font-bold text-2xl tracking-tighter">TICK: {tick}</span>
            </div>

            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-2xl border border-white/10">
              <span className="font-mono font-bold text-2xl tracking-tighter">ENTITIES: {entityCount}</span>
            </div>

            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-2xl border border-white/10">
              <span className="font-mono font-bold text-lg">{Math.ceil(enemyBaseHealth.current)}</span>
              <div className="w-48 h-3 bg-white/10 rounded-full overflow-hidden flex justify-end">
                <div
                  className="h-full bg-red-600 transition-all duration-300"
                  style={{ width: `${enemyBaseHealth.max > 0 ? (enemyBaseHealth.current / enemyBaseHealth.max) * 100 : 0}%` }}
                />
              </div>
            </div>
          </div>

          <div className="absolute bottom-6 left-1/2 -translate-x-1/2 pointer-events-none">
            <div className="flex items-center gap-6 bg-black/40 backdrop-blur-md px-6 py-3 rounded-2xl border border-white/10">
              <span className="text-white/40 text-xs font-bold uppercase tracking-widest">
                <span className="text-blue-400">●</span> Player Base
              </span>
              <span className="text-white/20">|</span>
              <span className="text-white/40 text-xs font-bold uppercase tracking-widest">
                <span className="text-red-500">●</span> Enemy Base
              </span>
              <span className="text-white/20">|</span>
              <span className="text-white/40 text-xs font-bold uppercase tracking-widest">
                <span className="text-yellow-400">●</span> Bullets
              </span>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
