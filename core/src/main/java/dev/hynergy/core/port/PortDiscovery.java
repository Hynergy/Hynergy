package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.joml.Vector3i;

import java.util.Objects;

/**
 * Finds compatible port pairs through a {@link PortWorldView}.
 *
 * <p>The world view selects the source and target layouts. Discovery applies
 * block rotations and checks each port's reach before it calls a resolver.
 * Discovery passes each result to the consumer and does not retain the result.</p>
 *
 * <p>Calls on this instance can run on different threads. Each caller must
 * satisfy the world view's thread restrictions. A nested call on this instance
 * on the same thread is not permitted.</p>
 */
public final class PortDiscovery {
    private final PortRegistry registry;
    private final ThreadLocal<Scratch> scratch = ThreadLocal.withInitial(Scratch::new);

    PortDiscovery(PortRegistry registry) {
        this.registry = Objects.requireNonNull(registry, "registry");
        if (!registry.isFrozen()) {
            throw new IllegalStateException("Port registry must be frozen before discovery is created");
        }
    }

    public <R> void discover(
            PortWorldView world,
            int x,
            int y,
            int z,
            int sourcePortId,
            PortDomain<R> domain,
            PortConnectionConsumer<? super R> out
    ) {
        scratch.get().discover(registry, world, x, y, z, sourcePortId, domain, out);
    }

    private static final class Scratch {
        private final Vector3i rotationScratch = new Vector3i();
        private boolean inUse;

        private PortRegistry registry;
        private PortWorldView world;
        private PortConnectionConsumer<Object> output;
        private int sourceX;
        private int sourceY;
        private int sourceZ;
        private PortDefinition<?, ?> sourcePort;
        private Object sourceProfile;
        private RotationTuple sourceRotation;
        private int sourceAnchorX;
        private int sourceAnchorY;
        private int sourceAnchorZ;

        private int targetX;
        private int targetY;
        private int targetZ;
        private RotationTuple targetRotation;

        <R> void discover(
                PortRegistry registry,
                PortWorldView world,
                int x,
                int y,
                int z,
                int sourcePortId,
                PortDomain<R> domain,
                PortConnectionConsumer<? super R> out
        ) {
            Objects.requireNonNull(world, "world");
            Objects.requireNonNull(domain, "domain");
            Objects.requireNonNull(out, "out");

            if (inUse) {
                throw new IllegalStateException("Port discovery is not reentrant on the same thread");
            }

            inUse = true;
            try {
                BlockPortDefinition sourceDefinition = world.portsAt(x, y, z);
                if (sourceDefinition == null) {
                    return;
                }

                PortDefinition<?, ?> sourcePort = sourceDefinition.port(sourcePortId);
                if (sourcePort == null) {
                    return;
                }

                if (sourcePort.standard().domain() != domain) {
                    throw new IllegalArgumentException(
                            "Source port belongs to domain " + sourcePort.standard()
                                                                         .domain()
                                                                         .id() + ", not " + domain.id()
                    );
                }

                RotationTuple rotation = Objects.requireNonNull(world.rotation(x, y, z), "source rotation");
                PortOffset sourceAnchor = sourcePort.anchor();
                rotationScratch.set(sourceAnchor.x(), sourceAnchor.y(), sourceAnchor.z());
                rotation.applyRotationTo(rotationScratch);
                sourceAnchorX = rotationScratch.x();
                sourceAnchorY = rotationScratch.y();
                sourceAnchorZ = rotationScratch.z();

                @SuppressWarnings("unchecked")
                PortConnectionConsumer<Object> rawOutput = (PortConnectionConsumer<Object>) out;

                this.registry = registry;
                this.world = world;
                this.output = rawOutput;
                this.sourceX = x;
                this.sourceY = y;
                this.sourceZ = z;
                this.sourcePort = sourcePort;
                this.sourceProfile = sourcePort.profile();
                this.sourceRotation = rotation;

                PortReach reach = sourcePort.reach();
                for (int index = 0; index < reach.size(); index++) {
                    visitCandidate(reach.x(index), reach.y(index), reach.z(index));
                }
            } finally {
                sourceProfile = null;
                sourcePort = null;
                sourceRotation = null;
                targetRotation = null;
                output = null;
                this.world = null;
                this.registry = null;
                inUse = false;
            }
        }

        private void visitCandidate(int ownerDx, int ownerDy, int ownerDz) {
            rotationScratch.set(ownerDx, ownerDy, ownerDz);
            sourceRotation.applyRotationTo(rotationScratch);

            int worldOwnerDx = rotationScratch.x();
            int worldOwnerDy = rotationScratch.y();
            int worldOwnerDz = rotationScratch.z();

            targetX = sourceX + worldOwnerDx;
            targetY = sourceY + worldOwnerDy;
            targetZ = sourceZ + worldOwnerDz;

            BlockPortDefinition targetDefinition = world.portsAt(targetX, targetY, targetZ);
            if (targetDefinition == null) {
                return;
            }

            targetRotation = Objects.requireNonNull(world.rotation(targetX, targetY, targetZ), "target rotation");

            for (int index = 0; index < targetDefinition.size(); index++) {
                PortDefinition<?, ?> targetPort = targetDefinition.portAt(index);
                PortRegistry.RuleSide<?, ?, ?> side =
                        registry.ruleSide(sourcePort.standard(), targetPort.standard());
                if (side == null) {
                    continue;
                }
                if (!targetCanReachSource(targetPort, worldOwnerDx, worldOwnerDy, worldOwnerDz)) {
                    continue;
                }
                resolveTarget(worldOwnerDx, worldOwnerDy, worldOwnerDz, targetPort, side);
            }
        }

        private boolean targetCanReachSource(
                PortDefinition<?, ?> targetPort,
                int sourceToTargetX,
                int sourceToTargetY,
                int sourceToTargetZ
        ) {
            PortReach reach = targetPort.reach();
            for (int index = 0; index < reach.size(); index++) {
                rotationScratch.set(reach.x(index), reach.y(index), reach.z(index));
                targetRotation.applyRotationTo(rotationScratch);
                if (rotationScratch.x() == -sourceToTargetX
                        && rotationScratch.y() == -sourceToTargetY
                        && rotationScratch.z() == -sourceToTargetZ) {
                    return true;
                }
            }
            return false;
        }

        @SuppressWarnings({"rawtypes", "unchecked"})
        private void resolveTarget(
                int ownerDx,
                int ownerDy,
                int ownerDz,
                PortDefinition<?, ?> targetPort,
                PortRegistry.RuleSide side
        ) {
            PortRegistry.Rule rule = side.rule();

            PortOffset targetAnchor = targetPort.anchor();
            rotationScratch.set(targetAnchor.x(), targetAnchor.y(), targetAnchor.z());
            targetRotation.applyRotationTo(rotationScratch);

            int anchorDx = ownerDx + rotationScratch.x() - sourceAnchorX;
            int anchorDy = ownerDy + rotationScratch.y() - sourceAnchorY;
            int anchorDz = ownerDz + rotationScratch.z() - sourceAnchorZ;

            Object targetProfile = targetPort.profile();
            Object resolution;
            boolean sourceIsResolverFirst = side.sourceIsFirst();
            if (sourceIsResolverFirst) {
                PortGeometry geometry = new PortGeometry(
                        ownerDx, ownerDy, ownerDz,
                        anchorDx, anchorDy, anchorDz,
                        sourceRotation, targetRotation
                );
                resolution = rule.resolver().resolve(sourceProfile, targetProfile, geometry);
            } else {
                PortGeometry geometry = new PortGeometry(
                        -ownerDx, -ownerDy, -ownerDz,
                        -anchorDx, -anchorDy, -anchorDz,
                        targetRotation, sourceRotation
                );
                resolution = rule.resolver().resolve(targetProfile, sourceProfile, geometry);
            }

            if (resolution != null) {
                output.accept(
                        targetX,
                        targetY,
                        targetZ,
                        targetPort.localId(),
                        resolution,
                        sourceIsResolverFirst
                );
            }
        }
    }
}
