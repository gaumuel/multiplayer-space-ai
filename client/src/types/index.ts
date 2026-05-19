export enum EntityType {
  Ship = 0,
  Bullet = 1,
  Base = 2,
  Pickup = 3,
  Obstacle = 4,
}

export interface EntityDelta {
  id: number;
  x: number;
  y: number;
  z: number;
  entity_type: EntityType;
  team: number | null;
  health: number | null;
  max_health: number | null;
}

export interface Snapshot {
  tick: number;
  entities: EntityDelta[];
}

export interface EntityState {
  id: number;
  x: number;
  y: number;
  z: number;
  entityType: EntityType;
  team: number | null;
  health: number | null;
  maxHealth: number | null;
  prevX: number;
  prevY: number;
  prevZ: number;
}
