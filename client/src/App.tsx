import { useEffect, useRef, useState, useCallback } from 'react';
import { mat4 } from 'gl-matrix';
import { WebGLRenderer } from './engine/renderer';
import { WtClient, ServerMessage } from './engine/wt-client';
import { GameState } from './game/state';
import { Snapshot } from './types';

const SERVER_URL = 'https://localhost:4433';

type AppState = 'connecting' | 'lobby' | 'waiting' | 'playing' | 'ended';

export default function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const gameStateRef = useRef(new GameState());
  const clientRef = useRef<WtClient | null>(null);
  const [appState, setAppState] = useState<AppState>('connecting');
  const [entityCount, setEntityCount] = useState(0);
  const [playerBaseHealth, setPlayerBaseHealth] = useState({ current: 0, max: 0 });
  const [enemyBaseHealth, setEnemyBaseHealth] = useState({ current: 0, max: 0 });
  const [tick, setTick] = useState(0);
  const [team, setTeam] = useState<number>(255);
  const [roomId, setRoomId] = useState('');
  const [rooms, setRooms] = useState<any[]>([]);
  const [selectedShipId, setSelectedShipId] = useState<number | null>(null);
  const selectedShipIdRef = useRef<number | null>(null);
  const autoFireRef = useRef(false);
  const [autoFire, setAutoFire] = useState(false);
  const [winner, setWinner] = useState<number | null>(null);
  const animFrameRef = useRef<number>(0);
  const keysRef = useRef<Set<string>>(new Set());
  const mouseWorldRef = useRef<{ x: number; y: number }>({ x: 0, y: 0 });

  const handleSnapshot = useCallback((snapshot: Snapshot) => {
    gameStateRef.current.applySnapshot(snapshot);
    setEntityCount(snapshot.entities.length);
    setTick(snapshot.tick);

    const pb = gameStateRef.current.getBaseHealth(0);
    if (pb) setPlayerBaseHealth(pb);
    const eb = gameStateRef.current.getBaseHealth(1);
    if (eb) setEnemyBaseHealth(eb);
  }, []);

  const handleControl = useCallback((msg: ServerMessage) => {
    switch (msg.type) {
      case 'RoomList':
        setRooms(msg.rooms || []);
        break;
      case 'RoomCreated':
      case 'RoomJoined':
        setRoomId(msg.room_id);
        setTeam(msg.team);
        setAppState('waiting');
        break;
      case 'GameStarted':
        setAppState('playing');
        break;
      case 'ShipSelected':
        setSelectedShipId(msg.ship_id);
        selectedShipIdRef.current = msg.ship_id;
        break;
      case 'NoShipAvailable':
        setSelectedShipId(null);
        selectedShipIdRef.current = null;
        break;
      case 'GameOver':
        setWinner(msg.winner_team);
        setAppState('ended');
        break;
      case 'JoinError':
        alert(msg.reason);
        break;
    }
  }, []);

  // Connect on mount
  useEffect(() => {
    const client = new WtClient(SERVER_URL);
    client.setOnSnapshot(handleSnapshot);
    client.setOnControl(handleControl);
    clientRef.current = client;

    client.connect().then(() => {
      setAppState('lobby');
      client.send({ type: 'ListRooms' });
    }).catch(() => {
      setAppState('connecting');
    });

    return () => { client.disconnect(); };
  }, [handleSnapshot, handleControl]);

  // Keyboard input
  useEffect(() => {
    if (appState !== 'playing' || team === 255) return;

    const onKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      keysRef.current.add(key);

      if (e.key === 'Tab') {
        e.preventDefault();
        clientRef.current?.send({ type: 'Command', command: { type: 'SelectNextShip' } });
        return;
      }
      if (e.key === ' ') {
        e.preventDefault();
        clientRef.current?.send({ type: 'Command', command: { type: 'Shoot' } });
        return;
      }
      if (key === 'f') {
        e.preventDefault();
        autoFireRef.current = !autoFireRef.current;
        setAutoFire(autoFireRef.current);
        clientRef.current?.send({ type: 'Command', command: { type: 'ToggleAutoFire' } });
        return;
      }
      if (key === '1') { clientRef.current?.send({ type: 'Command', command: { type: 'SetSpawnType', ship_type: 'Scout' } }); return; }
      if (key === '2') { clientRef.current?.send({ type: 'Command', command: { type: 'SetSpawnType', ship_type: 'Tank' } }); return; }
      if (key === '3') { clientRef.current?.send({ type: 'Command', command: { type: 'SetSpawnType', ship_type: 'Sniper' } }); return; }

      if ('wase'.includes(key)) {
        e.preventDefault();
        sendMovement();
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      keysRef.current.delete(key);
      if ('wase'.includes(key)) {
        sendMovement();
      }
    };

    const sendMovement = () => {
      const keys = keysRef.current;
      let dx = 0, dy = 0;
      if (keys.has('w')) dy += 1;
      if (keys.has('s')) dy -= 1;
      if (keys.has('a')) dx -= 1;
      if (keys.has('e')) dx += 1;

      if (dx === 0 && dy === 0) {
        clientRef.current?.send({ type: 'Command', command: { type: 'StopMove' } });
      } else {
        const len = Math.sqrt(dx * dx + dy * dy);
        clientRef.current?.send({ type: 'Command', command: { type: 'Move', dx: dx / len, dy: dy / len } });
      }
    };

    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('keyup', onKeyUp, true);

    const onMouseMove = (e: MouseEvent) => {
      if (!canvasRef.current || selectedShipIdRef.current === null) return;
      // Convert screen coords to world coords (approximate)
      const canvas = canvasRef.current;
      const cx = (e.clientX / canvas.width - 0.5) * 2;
      const cy = -(e.clientY / canvas.height - 0.5) * 2;
      // Camera at z=4500, FOV pi/4 → half-width ≈ 4500*tan(pi/8) ≈ 1864
      const worldX = cx * 1864 * (canvas.width / canvas.height);
      const worldY = cy * 1864;
      mouseWorldRef.current = { x: worldX, y: worldY };

      // Find selected ship position from game state
      const ship = gameStateRef.current.getEntity(selectedShipIdRef.current!);
      if (ship) {
        const dx = worldX - ship.x;
        const dy = worldY - ship.y;
        const len = Math.sqrt(dx * dx + dy * dy);
        if (len > 0) {
          clientRef.current?.send({ type: 'Command', command: { type: 'Aim', dx: dx / len, dy: dy / len } });
        }
      }
    };

    window.addEventListener('mousemove', onMouseMove);

    return () => {
      window.removeEventListener('keydown', onKeyDown, true);
      window.removeEventListener('keyup', onKeyUp, true);
      window.removeEventListener('mousemove', onMouseMove);
    };
  }, [appState, team]);

  // Render loop
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    rendererRef.current = new WebGLRenderer(canvas);

    const loop = (time: number) => {
      const aspect = canvas.width / canvas.height;
      const projectionMatrix = mat4.create();
      mat4.perspective(projectionMatrix, Math.PI / 4, aspect, 10, 20000);
      const viewMatrix = mat4.create();
      mat4.lookAt(viewMatrix, [0, 0, 4500], [0, 0, 0], [0, 1, 0]);

      gameStateRef.current.interpolate(1.0);
      const { positions, colors, sizes, types, count } = gameStateRef.current.getInterpolatedPositions();
      rendererRef.current?.render(positions, colors, sizes, types, count, viewMatrix, projectionMatrix, time / 1000);
      animFrameRef.current = requestAnimationFrame(loop);
    };

    animFrameRef.current = requestAnimationFrame(loop);
    const handleResize = () => { canvas.width = window.innerWidth; canvas.height = window.innerHeight; };
    window.addEventListener('resize', handleResize);
    return () => { cancelAnimationFrame(animFrameRef.current); window.removeEventListener('resize', handleResize); };
  }, []);

  // Lobby actions
  const createRoom = (mode: string) => {
    clientRef.current?.send({ type: 'CreateRoom', mode });
  };

  const joinRoom = (id: string, role: string) => {
    clientRef.current?.send({ type: 'JoinRoom', room_id: id, role });
  };

  const refreshRooms = () => {
    clientRef.current?.send({ type: 'ListRooms' });
  };

  return (
    <div className="relative w-full h-screen bg-slate-950 overflow-hidden font-sans text-white">
      <canvas ref={canvasRef} className="absolute inset-0 w-full h-full" />

      {/* CONNECTING */}
      {appState === 'connecting' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/90">
          <h1 className="text-4xl font-bold mb-4">Connecting...</h1>
        </div>
      )}

      {/* LOBBY */}
      {appState === 'lobby' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/90">
          <h1 className="text-6xl font-black italic tracking-tighter uppercase mb-8 text-transparent bg-clip-text bg-gradient-to-b from-white to-white/20">
            STRIKE 2.5D
          </h1>

          <div className="flex gap-4 mb-8">
            <button onClick={() => createRoom('HumanVsAI')} className="bg-blue-600 hover:bg-blue-500 px-6 py-3 rounded-lg font-bold">
              Play vs AI
            </button>
            <button onClick={() => createRoom('HumanVsHuman')} className="bg-green-600 hover:bg-green-500 px-6 py-3 rounded-lg font-bold">
              Create PvP Room
            </button>
            <button onClick={() => createRoom('AIVsAI')} className="bg-purple-600 hover:bg-purple-500 px-6 py-3 rounded-lg font-bold">
              Watch AI vs AI
            </button>
          </div>

          <div className="bg-black/40 backdrop-blur-md p-4 rounded-xl border border-white/10 w-96">
            <div className="flex justify-between items-center mb-3">
              <h2 className="font-bold text-lg">Open Rooms</h2>
              <button onClick={refreshRooms} className="text-sm text-white/60 hover:text-white">Refresh</button>
            </div>
            {rooms.length === 0 ? (
              <p className="text-white/40 text-sm">No rooms available</p>
            ) : (
              <div className="space-y-2">
                {rooms.map((r: any) => (
                  <div key={r.id} className="flex justify-between items-center bg-white/5 p-2 rounded">
                    <span className="font-mono text-sm">{r.id} ({r.mode}) - {r.state}</span>
                    <div className="flex gap-2">
                      <button onClick={() => joinRoom(r.id, 'Player')} className="text-xs bg-blue-600 px-2 py-1 rounded">Join</button>
                      <button onClick={() => joinRoom(r.id, 'Spectator')} className="text-xs bg-gray-600 px-2 py-1 rounded">Watch</button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* WAITING */}
      {appState === 'waiting' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/90">
          <h1 className="text-4xl font-bold mb-4">Waiting for opponent...</h1>
          <p className="text-white/60">Room: <span className="font-mono">{roomId}</span> | Team: {team === 0 ? 'Blue' : team === 1 ? 'Red' : 'Spectator'}</p>
        </div>
      )}

      {/* PLAYING HUD */}
      {appState === 'playing' && (
        <>
          <div className="absolute top-6 left-6 flex flex-col gap-3 pointer-events-none">
            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-xl border border-white/10">
              <div className="w-3 h-3 rounded-full bg-emerald-400 animate-pulse" />
              <span className="font-mono font-bold text-sm text-emerald-400">LIVE</span>
              <span className="font-mono text-xs text-white/40 ml-2">Room: {roomId}</span>
            </div>
            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-xl border border-white/10">
              <div className="w-48 h-3 bg-white/10 rounded-full overflow-hidden">
                <div className="h-full bg-blue-400 transition-all duration-300" style={{ width: `${playerBaseHealth.max > 0 ? (playerBaseHealth.current / playerBaseHealth.max) * 100 : 0}%` }} />
              </div>
              <span className="font-mono font-bold text-lg">{Math.ceil(playerBaseHealth.current)}</span>
            </div>
            {selectedShipId !== null && (
              <div className="bg-black/40 backdrop-blur-md p-2 rounded-xl border border-yellow-400/30 text-yellow-400 font-mono text-xs">
                Ship selected | {autoFire ? 'AUTO-FIRE ON' : 'Manual aim (F to toggle)'}
              </div>
            )}
          </div>

          <div className="absolute top-6 right-6 flex flex-col items-end gap-3 pointer-events-none">
            <div className="bg-black/40 backdrop-blur-md p-3 rounded-xl border border-white/10">
              <span className="font-mono font-bold text-xl">TICK: {tick}</span>
            </div>
            <div className="bg-black/40 backdrop-blur-md p-3 rounded-xl border border-white/10">
              <span className="font-mono font-bold text-xl">ENTITIES: {entityCount}</span>
            </div>
            <div className="flex items-center gap-3 bg-black/40 backdrop-blur-md p-3 rounded-xl border border-white/10">
              <span className="font-mono font-bold text-lg">{Math.ceil(enemyBaseHealth.current)}</span>
              <div className="w-48 h-3 bg-white/10 rounded-full overflow-hidden flex justify-end">
                <div className="h-full bg-red-600 transition-all duration-300" style={{ width: `${enemyBaseHealth.max > 0 ? (enemyBaseHealth.current / enemyBaseHealth.max) * 100 : 0}%` }} />
              </div>
            </div>
          </div>

          {team !== 255 && (
            <div className="absolute bottom-6 left-1/2 -translate-x-1/2 pointer-events-none">
              <div className="bg-black/40 backdrop-blur-md px-6 py-3 rounded-xl border border-white/10 font-mono text-xs text-white/50">
                WASE: Move | Mouse: Aim | Space: Shoot | F: Auto-fire | Tab: Select Ship | 1/2/3: Scout/Tank/Sniper
              </div>
            </div>
          )}
        </>
      )}

      {/* GAME OVER */}
      {appState === 'ended' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-slate-950/80">
          <h1 className="text-6xl font-black italic uppercase mb-4">
            {winner === team ? <span className="text-emerald-400">VICTORY</span> : <span className="text-red-400">DEFEAT</span>}
          </h1>
          <p className="text-white/60 mb-8">Team {winner === 0 ? 'Blue' : 'Red'} wins!</p>
          <button onClick={() => { setAppState('lobby'); refreshRooms(); }} className="bg-white text-black px-8 py-3 rounded-full font-bold hover:scale-105 transition-all">
            Back to Lobby
          </button>
        </div>
      )}
    </div>
  );
}
