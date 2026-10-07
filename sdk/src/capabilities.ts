export type RouteStatus = "pending-verification" | "unavailable" | "verified";
export type Capability = "lend" | "supplyCollateral" | "borrow" | "repay" | "withdraw";

export interface ManifestRoute {
  id: string;
  protocol: string;
  network: string;
  protocolVersion: string;
  enabled: boolean;
  status: RouteStatus;
  reason: string;
  capabilities: Record<Capability, boolean>;
  riskReads: string[];
}

export interface Manifest {
  schemaVersion: number;
  environment: "mainnet" | "testnet";
  routes: ManifestRoute[];
}

export interface RouteAvailability {
  id: string;
  protocol: string;
  network: string;
  usable: boolean;
  reason: string | null;
  capabilities: Capability[];
}

/** A route is usable only when enabled AND verified; otherwise its reason is surfaced. */
export function routeAvailability(manifest: Manifest): RouteAvailability[] {
  if (manifest.schemaVersion !== 1) throw new Error("UnsupportedSchemaVersion");
  return manifest.routes.map((r) => {
    const usable = r.enabled && r.status === "verified";
    return {
      id: r.id,
      protocol: r.protocol,
      network: r.network,
      usable,
      reason: usable ? null : r.reason || `route ${r.status}`,
      capabilities: (Object.keys(r.capabilities) as Capability[]).filter((c) => r.capabilities[c]),
    };
  });
}

export function canPerform(manifest: Manifest, routeId: string, capability: Capability): boolean {
  const route = routeAvailability(manifest).find((r) => r.id === routeId);
  return !!route && route.usable && route.capabilities.includes(capability);
}
