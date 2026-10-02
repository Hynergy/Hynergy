package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.Rotation;
import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.joml.Vector3i;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class PortRotationTest {
    private record Profile(String name) {}

    @Test
    void candidateOwnerOffsetRotatesWithSourceBlock() {
        RotationTuple sourceRotation = RotationTuple.of(Rotation.Ninety, Rotation.None);
        Fixture fixture = Fixture.create(PortOffset.ZERO, PortOffset.ZERO);
        fixture.world.put(0, 0, 0, 1, sourceRotation);
        PortOffset rotated = rotate(new PortOffset(1, 0, 0), sourceRotation);
        fixture.world.put(rotated.x(), rotated.y(), rotated.z(), 2, RotationTuple.NONE);

        List<PortGeometry> geometries = fixture.discover();

        assertEquals(1, geometries.size());
        assertEquals(rotated.x(), geometries.getFirst().ownerDx());
        assertEquals(rotated.y(), geometries.getFirst().ownerDy());
        assertEquals(rotated.z(), geometries.getFirst().ownerDz());
    }

    @Test
    void sourceAndTargetAnchorsAreRotatedBeforeAnchorDeltaIsBuilt() {
        RotationTuple sourceRotation = RotationTuple.of(Rotation.Ninety, Rotation.None);
        RotationTuple targetRotation = RotationTuple.of(Rotation.TwoSeventy, Rotation.None);
        PortOffset sourceAnchor = new PortOffset(1, 0, 0);
        PortOffset targetAnchor = new PortOffset(0, 0, 1);
        Fixture fixture = Fixture.create(sourceAnchor, targetAnchor);
        fixture.world.put(0, 0, 0, 1, sourceRotation);
        PortOffset ownerOffset = rotate(new PortOffset(1, 0, 0), sourceRotation);
        fixture.world.put(ownerOffset.x(), ownerOffset.y(), ownerOffset.z(), 2, targetRotation);

        PortGeometry geometry = fixture.discover().getFirst();
        PortOffset rotatedSourceAnchor = rotate(sourceAnchor, sourceRotation);
        PortOffset rotatedTargetAnchor = rotate(targetAnchor, targetRotation);

        assertEquals(ownerOffset.x() + rotatedTargetAnchor.x() - rotatedSourceAnchor.x(), geometry.anchorDx());
        assertEquals(ownerOffset.y() + rotatedTargetAnchor.y() - rotatedSourceAnchor.y(), geometry.anchorDy());
        assertEquals(ownerOffset.z() + rotatedTargetAnchor.z() - rotatedSourceAnchor.z(), geometry.anchorDz());
    }

    @Test
    void targetAtOldUnrotatedOffsetDoesNotMatchAfterRotation() {
        RotationTuple sourceRotation = RotationTuple.of(Rotation.Ninety, Rotation.None);
        Fixture fixture = Fixture.create(PortOffset.ZERO, PortOffset.ZERO);
        fixture.world.put(0, 0, 0, 1, sourceRotation);
        fixture.world.put(1, 0, 0, 2, RotationTuple.NONE);

        PortOffset rotated = rotate(new PortOffset(1, 0, 0), sourceRotation);
        if (rotated.equals(new PortOffset(1, 0, 0))) {
            throw new AssertionError("Chosen test rotation did not move the search offset");
        }

        assertTrue(fixture.discover().isEmpty());
    }

    private static PortOffset rotate(PortOffset offset, RotationTuple rotation) {
        Vector3i vector = new Vector3i(offset.x(), offset.y(), offset.z());
        rotation.applyRotationTo(vector);
        return new PortOffset(vector.x, vector.y, vector.z);
    }

    private static final class Fixture {
        final TestWorld world = new TestWorld();
        final PortModule module;
        final PortDomain<PortGeometry> domain;

        private Fixture(PortModule module, PortDomain<PortGeometry> domain) {
            this.module = module;
            this.domain = domain;
        }

        static Fixture create(PortOffset sourceAnchor, PortOffset targetAnchor) {
            PortModule module = new PortModule();
            PortDomain<PortGeometry> domain = module.registerDomain("test:rotation");
            PortStandard<Profile, PortGeometry> standard = module.registerStandard(
                "test:rotation-standard",
                domain,
                Profile.class,
                (first, second, geometry) -> geometry
            );
            PortModuleTestAccess.freeze(module);
            module.setBlockPorts(1, BlockPortDefinition.of(
                new PortDefinition<>(
                    0, sourceAnchor, PortReach.single(1, 0, 0), standard, new Profile("source")
                )
            ));
            // Reach is the inverse in the target's local coordinates. For tests with
            // target rotation we derive it exactly from Hytale's transform.
            module.setBlockPorts(2, BlockPortDefinition.of(
                new PortDefinition<>(
                    1, targetAnchor, PortReach.of(
                        new PortOffset(-1, 0, 0),
                        new PortOffset(1, 0, 0),
                        new PortOffset(0, 0, -1),
                        new PortOffset(0, 0, 1)
                    ), standard, new Profile("target")
                )
            ));
            return new Fixture(module, domain);
        }

        List<PortGeometry> discover() {
            List<PortGeometry> geometries = new ArrayList<>();
            module.discovery().discover(
                world,
                0,
                0,
                0,
                0,
                domain,
                (x, y, z, port, geometry, sourceFirst) -> geometries.add(geometry)
            );
            return geometries;
        }
    }

    private static final class TestWorld implements PortWorldView {
        private final Map<Position, Block> blocks = new HashMap<>();

        void put(int x, int y, int z, int blockTypeId, RotationTuple rotation) {
            blocks.put(new Position(x, y, z), new Block(blockTypeId, rotation));
        }

        @Override
        public int blockTypeId(int x, int y, int z) {
            Block block = blocks.get(new Position(x, y, z));
            return block == null ? -1 : block.id();
        }

        @Override
        public RotationTuple rotation(int x, int y, int z) {
            return blocks.get(new Position(x, y, z)).rotation();
        }
    }

    private record Position(int x, int y, int z) {}
    private record Block(int id, RotationTuple rotation) {}
}
