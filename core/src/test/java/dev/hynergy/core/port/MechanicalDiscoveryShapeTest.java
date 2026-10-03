package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.joml.Vector3i;
import org.jspecify.annotations.Nullable;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MechanicalDiscoveryShapeTest {
    private enum GearSize {SMALL, LARGE}

    private enum Axis {X, Y, Z}

    private record GearProfile(int teeth, GearSize size, Axis axis) {
    }

    private record MechanicalRelation(int numerator, int denominator) {
    }

    @Test
    void smallAndLargeGearCanMeshAtDiagonalOffset() {
        Fixture fixture = Fixture.create(
                new GearProfile(12, GearSize.SMALL, Axis.Z),
                new GearProfile(36, GearSize.LARGE, Axis.Z)
        );
        assertEquals(1, fixture.discover().size());
    }

    @Test
    void smallAndSmallGearDoNotMeshAtThatDiagonalOffset() {
        Fixture fixture = Fixture.create(
                new GearProfile(12, GearSize.SMALL, Axis.Z),
                new GearProfile(12, GearSize.SMALL, Axis.Z)
        );
        assertTrue(fixture.discover().isEmpty());
    }

    @Test
    void largeAndLargeGearDoNotMeshAtThatDiagonalOffset() {
        Fixture fixture = Fixture.create(
                new GearProfile(36, GearSize.LARGE, Axis.Z),
                new GearProfile(36, GearSize.LARGE, Axis.Z)
        );
        assertTrue(fixture.discover().isEmpty());
    }

    @Test
    void validGearResolutionCarriesSignedRatio() {
        MechanicalRelation relation = Fixture.create(
                new GearProfile(12, GearSize.SMALL, Axis.Z),
                new GearProfile(36, GearSize.LARGE, Axis.Z)
        ).discover().getFirst();
        assertEquals(-1, relation.numerator());
        assertEquals(3, relation.denominator());
    }

    private static MechanicalRelation resolve(
            GearProfile first,
            GearProfile second,
            PortGeometry geometry
    ) {
        if (Math.abs(geometry.ownerDx()) != 1 || Math.abs(geometry.ownerDy()) != 1 || geometry.ownerDz() != 0) {
            return null;
        }
        if (first.size() == second.size()) {
            return null;
        }
        if (rotatedAxis(first.axis(), geometry.firstRotation()) != rotatedAxis(second.axis(), geometry.secondRotation())) {
            return null;
        }

        int divisor = gcd(first.teeth(), second.teeth());
        return new MechanicalRelation(-first.teeth() / divisor, second.teeth() / divisor);
    }

    private static Axis rotatedAxis(Axis axis, RotationTuple rotation) {
        Vector3i vector = switch (axis) {
            case X -> new Vector3i(1, 0, 0);
            case Y -> new Vector3i(0, 1, 0);
            case Z -> new Vector3i(0, 0, 1);
        };
        rotation.applyRotationTo(vector);
        if (vector.x != 0) return Axis.X;
        if (vector.y != 0) return Axis.Y;
        return Axis.Z;
    }

    private static int gcd(int first, int second) {
        int a = Math.abs(first);
        int b = Math.abs(second);
        while (b != 0) {
            int next = a % b;
            a = b;
            b = next;
        }
        return a;
    }

    private static final class Fixture {
        final PortModule module;
        final PortDomain<MechanicalRelation> domain;
        final TestWorld world;

        private Fixture(PortModule module, PortDomain<MechanicalRelation> domain) {
            this.module = module;
            this.world = new TestWorld(module);
            this.domain = domain;
            world.put(0, 0, 0, 1);
            world.put(1, 1, 0, 2);
        }

        static Fixture create(GearProfile source, GearProfile target) {
            PortModule module = new PortModule();
            PortDomain<MechanicalRelation> domain = module.registerDomain("test:mechanical");
            PortStandard<GearProfile, MechanicalRelation> standard = module.registerStandard(
                    "test:spur-gear",
                    domain,
                    GearProfile.class,
                    MechanicalDiscoveryShapeTest::resolve
            );
            PortModuleTestAccess.freeze(module);
            PortReach diagonal = PortReach.of(
                    new PortOffset(1, 1, 0), new PortOffset(1, -1, 0),
                    new PortOffset(-1, 1, 0), new PortOffset(-1, -1, 0)
            );
            module.setBlockPorts(1, BlockPortDefinition.of(
                    new PortDefinition<>(0, PortOffset.ZERO, diagonal, standard, source)
            ));
            module.setBlockPorts(2, BlockPortDefinition.of(
                    new PortDefinition<>(1, PortOffset.ZERO, diagonal, standard, target)
            ));
            return new Fixture(module, domain);
        }

        List<MechanicalRelation> discover() {
            List<MechanicalRelation> relations = new ArrayList<>();
            module.discovery().discover(
                    world, 0, 0, 0, 0, domain,
                    (x, y, z, targetPort, relation, sourceFirst) -> relations.add(relation)
            );
            return relations;
        }
    }

    private static final class TestWorld implements PortWorldView {
        private final PortModule module;

        private final Map<Position, Integer> blocks = new HashMap<>();

        TestWorld(PortModule module) {
            this.module = module;
        }

        void put(int x, int y, int z, int id) {
            blocks.put(new Position(x, y, z), id);
        }

        @Override
        public @Nullable BlockPortDefinition portsAt(int x, int y, int z) {
            return module.blockPorts(blocks.getOrDefault(new Position(x, y, z), -1));
        }

        @Override
        public RotationTuple rotation(int x, int y, int z) {
            return RotationTuple.NONE;
        }
    }

    private record Position(int x, int y, int z) {
    }
}
