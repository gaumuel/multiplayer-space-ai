import { Snapshot, EntityState, EntityType } from '../types';

export class GameState {
  private entities = new Map<number, EntityState>();
  private lastSnapshot: Snapshot | null = null;
  private currentSnapshot: Snapshot | null = null;
  private interpolationFactor = 0;

  getEntities(): EntityState[] {
    return Array.from(this.entities.values());
  }

  getEntityCount(): number {
    return this.entities.size;
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

  getInterpolatedPositions(): {
    positions: Float32Array;
    colors: Float32Array;
    sizes: Float32Array;
    types: Float32Array;
    count: number;
  } {
    const t = this.interpolationFactor;
    const count = this.entities.size;
    const positions = new Float32Array(count * 3);
    const colors = new Float32Array(count * 4);
    const sizes = new Float32Array(count);
    const types = new Float32Array(count);

    let idx = 0;
    for (const entity of this.entities.values()) {
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
          colors[idx * 4] = 0.5;
          colors[idx * 4 + 1] = 0.5;
          colors[idx * 4 + 2] = 0.5;
          sizes[idx] = 10;
      }

      colors[idx * 4 + 3] = 1.0;
      types[idx] = entity.entityType;
      idx++;
    }

    return { positions, colors, sizes, types, count };
  }
}
