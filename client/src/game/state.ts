import { Snapshot, EntityState, EntityType } from '../types';

const MAX_ENTITIES = 2048;
const MAX_PARTICLES = 500;

interface Particle {
  x: number; y: number; z: number;
  vx: number; vy: number;
  life: number; maxLife: number;
  r: number; g: number; b: number;
  size: number;
}

export class GameState {
  private entities = new Map<number, EntityState>();
  private lastSnapshot: Snapshot | null = null;
  private currentSnapshot: Snapshot | null = null;
  private interpolationFactor = 0;
  private particles: Particle[] = [];
  private lastBaseHealth = new Map<number, number>(); // team -> last known health
  private lastBasePos = new Map<number, { x: number; y: number }>(); // team -> position

  // Pre-allocated render buffers
  private _positions = new Float32Array((MAX_ENTITIES + MAX_PARTICLES) * 3);
  private _colors = new Float32Array((MAX_ENTITIES + MAX_PARTICLES) * 4);
  private _sizes = new Float32Array(MAX_ENTITIES + MAX_PARTICLES);
  private _types = new Float32Array(MAX_ENTITIES + MAX_PARTICLES);

  getEntities(): EntityState[] {
    return Array.from(this.entities.values());
  }

  getEntityCount(): number {
    return this.entities.size;
  }

  getEntity(id: number): EntityState | undefined {
    return this.entities.get(id);
  }

  getBaseHealth(team: number): { current: number; max: number } | null {
    for (const entity of this.entities.values()) {
      if (entity.entityType === EntityType.Base && entity.team === team) {
        return { current: entity.health ?? 0, max: entity.maxHealth ?? 0 };
      }
    }
    return null;
  }

  applySnapshot(snapshot: Snapshot) {
    this.lastSnapshot = this.currentSnapshot;
    this.currentSnapshot = snapshot;
    this.interpolationFactor = 0;

    // Track base health for explosion detection
    for (const delta of snapshot.entities) {
      if (delta.entity_type === EntityType.Base && delta.team !== null) {
        this.lastBasePos.set(delta.team, { x: delta.x, y: delta.y });
        const prevHealth = this.lastBaseHealth.get(delta.team);
        if (prevHealth !== undefined && prevHealth > 0 && (delta.health ?? 0) <= 0) {
          // Base just died — trigger explosion!
          this.spawnExplosion(delta.x, delta.y, delta.team);
        }
        this.lastBaseHealth.set(delta.team, delta.health ?? 0);
      }
    }

    const nextIds = new Set<number>();

    for (const delta of snapshot.entities) {
      nextIds.add(delta.id);

      const existing = this.entities.get(delta.id);
      if (existing) {
        existing.prevX = existing.x;
        existing.prevY = existing.y;
        existing.prevZ = existing.z;
        existing.x = delta.x;
        existing.y = delta.y;
        existing.z = delta.z;
        existing.health = delta.health;
        existing.maxHealth = delta.max_health;
      } else {
        this.entities.set(delta.id, {
          id: delta.id,
          x: delta.x,
          y: delta.y,
          z: delta.z,
          entityType: delta.entity_type,
          team: delta.team,
          health: delta.health,
          maxHealth: delta.max_health,
          prevX: delta.x,
          prevY: delta.y,
          prevZ: delta.z,
        });
      }
    }

    for (const id of this.entities.keys()) {
      if (!nextIds.has(id)) {
        this.entities.delete(id);
      }
    }
  }

  interpolate(factor: number) {
    this.interpolationFactor = factor;
  }

  updateParticles(dt: number) {
    for (let i = this.particles.length - 1; i >= 0; i--) {
      const p = this.particles[i];
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.life -= dt;
      p.size *= 0.97;
      if (p.life <= 0) {
        this.particles.splice(i, 1);
      }
    }
  }

  triggerBaseExplosion(team: number) {
    const pos = this.lastBasePos.get(team);
    if (pos) {
      this.spawnExplosion(pos.x, pos.y, team);
      // Mark base as dead so we stop rendering it
      this.deadBases.add(team);
    }
  }

  resetExplosions() {
    this.deadBases.clear();
    this.particles = [];
    this.lastBaseHealth.clear();
  }

  getObstacleRects(): { x: number; y: number; w: number; h: number; r: number; g: number; b: number }[] {
    const rects: { x: number; y: number; w: number; h: number; r: number; g: number; b: number }[] = [];
    for (const entity of this.entities.values()) {
      if (entity.entityType === EntityType.Obstacle && entity.team === 0) {
        const w = entity.health ?? 100;
        const h = entity.maxHealth ?? 100;
        rects.push({ x: entity.x, y: entity.y, w, h, r: 0.3, g: 0.3, b: 0.38 });
      }
    }
    return rects;
  }

  private deadBases = new Set<number>();

  private spawnExplosion(x: number, y: number, team: number) {
    const count = 300;
    for (let i = 0; i < count; i++) {
      const angle = Math.random() * Math.PI * 2;
      const speed = 50 + Math.random() * 900;
      const life = 1.0 + Math.random() * 2.5;
      const size = 10 + Math.random() * 50;

      let r: number, g: number, b: number;
      const rnd = Math.random();
      if (rnd < 0.25) {
        // White/yellow hot core
        r = 1.0; g = 0.85 + Math.random() * 0.15; b = 0.2 + Math.random() * 0.5;
      } else if (rnd < 0.45) {
        // Orange/fire
        r = 1.0; g = 0.4 + Math.random() * 0.3; b = 0.05;
      } else if (team === 0) {
        // Blue team
        r = 0.05 + Math.random() * 0.2; g = 0.2 + Math.random() * 0.5; b = 0.7 + Math.random() * 0.3;
      } else {
        // Red team
        r = 0.7 + Math.random() * 0.3; g = 0.05 + Math.random() * 0.2; b = 0.05 + Math.random() * 0.15;
      }

      this.particles.push({
        x, y, z: 10,
        vx: Math.cos(angle) * speed,
        vy: Math.sin(angle) * speed,
        life, maxLife: life,
        r, g, b,
        size,
      });
    }
  }

  getInterpolatedPositions(): {
    positions: Float32Array;
    colors: Float32Array;
    sizes: Float32Array;
    types: Float32Array;
    count: number;
  } {
    const t = this.interpolationFactor;
    const count = this.entities.size;

    // Grow buffers if needed (rare)
    if (count > this._positions.length / 3) {
      const newSize = count * 2;
      this._positions = new Float32Array(newSize * 3);
      this._colors = new Float32Array(newSize * 4);
      this._sizes = new Float32Array(newSize);
      this._types = new Float32Array(newSize);
    }

    const positions = this._positions;
    const colors = this._colors;
    const sizes = this._sizes;
    const types = this._types;

    let idx = 0;
    for (const entity of this.entities.values()) {
      // Skip dead bases
      if (entity.entityType === EntityType.Base && this.deadBases.has(entity.team ?? -1)) {
        continue;
      }

      // Skip rectangular obstacles — they're rendered separately as rects
      // Circle obstacles (team=1) are rendered as point sprites
      if (entity.entityType === EntityType.Obstacle && entity.team === 0) {
        continue;
      }

      positions[idx * 3] = entity.prevX + (entity.x - entity.prevX) * t;
      positions[idx * 3 + 1] = entity.prevY + (entity.y - entity.prevY) * t;
      positions[idx * 3 + 2] = entity.prevZ + (entity.z - entity.prevZ) * t;

      switch (entity.entityType) {
        case EntityType.Ship:
          if (entity.team === 0) {
            colors[idx * 4] = 0.2;
            colors[idx * 4 + 1] = 0.6;
            colors[idx * 4 + 2] = 1.0;
          } else {
            colors[idx * 4] = 1.0;
            colors[idx * 4 + 1] = 0.3;
            colors[idx * 4 + 2] = 0.3;
          }
          sizes[idx] = 20;
          break;

        case EntityType.Bullet:
          colors[idx * 4] = 1.0;
          colors[idx * 4 + 1] = 0.9;
          colors[idx * 4 + 2] = 0.3;
          sizes[idx] = 8;
          break;

        case EntityType.Base:
          if (entity.team === 0) {
            colors[idx * 4] = 0.2;
            colors[idx * 4 + 1] = 0.8;
            colors[idx * 4 + 2] = 1.0;
          } else {
            colors[idx * 4] = 1.0;
            colors[idx * 4 + 1] = 0.2;
            colors[idx * 4 + 2] = 0.2;
          }
          sizes[idx] = 200;
          break;

        default:
          // Circle obstacles (team=1) and other entities
          if (entity.entityType === EntityType.Obstacle) {
            colors[idx * 4] = 0.4;
            colors[idx * 4 + 1] = 0.4;
            colors[idx * 4 + 2] = 0.45;
            const diameter = entity.health ?? 100;
            sizes[idx] = diameter;
          } else {
            colors[idx * 4] = 0.5;
            colors[idx * 4 + 1] = 0.5;
            colors[idx * 4 + 2] = 0.5;
            sizes[idx] = 10;
          }
      }

      colors[idx * 4 + 3] = 1.0;
      types[idx] = entity.entityType;
      idx++;
    }

    // Add particles to render output
    for (const p of this.particles) {
      const alpha = Math.max(0, p.life / p.maxLife);
      positions[idx * 3] = p.x;
      positions[idx * 3 + 1] = p.y;
      positions[idx * 3 + 2] = p.z;
      colors[idx * 4] = p.r;
      colors[idx * 4 + 1] = p.g;
      colors[idx * 4 + 2] = p.b;
      colors[idx * 4 + 3] = alpha;
      sizes[idx] = p.size * alpha;
      types[idx] = 1; // bullet type for glow effect
      idx++;
    }

    const totalCount = idx;
    return { positions, colors, sizes, types, count: totalCount };
  }
}
