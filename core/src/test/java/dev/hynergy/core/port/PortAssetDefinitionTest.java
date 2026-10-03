package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.jspecify.annotations.Nullable;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

final class PortAssetDefinitionTest {
    private record Profile(String name) {
    }

    private record Result(String value) {
    }

    @Test
    void blockPortsCanBeInstalledAfterProtocolFreezeAndDiscovered() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard",
                domain,
                Profile.class,
                (first, second, geometry) -> new Result(first.name() + ":" + second.name())
        );
        PortModuleTestAccess.freeze(module);

        module.setBlockPorts(1, BlockPortDefinition.of(
                new PortDefinition<>(0, new PortOffset(0, 0, 0), PortReach.single(1, 0, 0), standard, new Profile("source"))
        ));
        module.setBlockPorts(2, BlockPortDefinition.of(
                new PortDefinition<>(5, new PortOffset(0, 0, 0), PortReach.single(-1, 0, 0), standard, new Profile("target"))
        ));

        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);
        List<Result> results = new ArrayList<>();

        module.discovery().discover(
                world, 0, 0, 0, 0, domain,
                (x, y, z, portId, result, sourceFirst) -> results.add(result)
        );

        assertEquals(List.of(new Result("source:target")), results);
    }

    @Test
    void replacingAndClearingRuntimeBlockPortsTakesEffectImmediately() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result(second.name())
        );
        PortModuleTestAccess.freeze(module);

        BlockPortDefinition source = BlockPortDefinition.of(
                new PortDefinition<>(0, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile("source"))
        );
        module.setBlockPorts(1, source);
        module.setBlockPorts(2, BlockPortDefinition.of(
                new PortDefinition<>(4, PortOffset.ZERO, PortReach.single(-1, 0, 0), standard, new Profile("first"))
        ));

        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);
        assertEquals(List.of(new Result("first")), discover(module, world, domain));

        module.setBlockPorts(2, BlockPortDefinition.of(
                new PortDefinition<>(4, PortOffset.ZERO, PortReach.single(-1, 0, 0), standard, new Profile("replacement"))
        ));
        assertEquals(List.of(new Result("replacement")), discover(module, world, domain));

        module.clearBlockPorts(2);
        assertTrue(discover(module, world, domain).isEmpty());
        assertNull(module.blockPorts(2));
    }

    @Test
    void bothPortReachesMustContainTheRelationship() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result("connected")
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(
                new PortDefinition<>(0, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile("source"))
        ));
        module.setBlockPorts(2, BlockPortDefinition.of(
                new PortDefinition<>(1, PortOffset.ZERO, PortReach.single(0, 1, 0), standard, new Profile("target"))
        ));

        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);

        assertTrue(discover(module, world, domain).isEmpty());
    }

    @Test
    void runtimeDefinitionRejectsStandardsFromAnotherPortModule() {
        PortModule owner = new PortModule();
        PortDomain<Result> ownerDomain = owner.registerDomain("test:owner-domain");
        owner.registerStandard(
                "test:owner-standard", ownerDomain, Profile.class,
                (first, second, geometry) -> new Result("owner")
        );

        PortModule foreign = new PortModule();
        PortDomain<Result> foreignDomain = foreign.registerDomain("test:foreign-domain");
        PortStandard<Profile, Result> foreignStandard = foreign.registerStandard(
                "test:foreign-standard", foreignDomain, Profile.class,
                (first, second, geometry) -> new Result("foreign")
        );
        BlockPortDefinition definition = BlockPortDefinition.of(
                new PortDefinition<>(
                        0, PortOffset.ZERO, PortReach.single(1, 0, 0), foreignStandard, new Profile("foreign")
                )
        );

        assertThrows(IllegalArgumentException.class, () -> owner.setBlockPorts(1, definition));
    }

    @Test
    void blockDefinitionRejectsInvalidPortDefinitions() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result("connected")
        );

        assertThrows(IllegalArgumentException.class, () ->
                new PortDefinition<>(-1, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile("bad"))
        );
        PortDefinition<Profile, Result> first =
                new PortDefinition<>(3, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile("a"));
        PortDefinition<Profile, Result> second =
                new PortDefinition<>(3, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile("b"));
        assertThrows(IllegalArgumentException.class, () -> BlockPortDefinition.of(first, second));
    }

    private static List<Result> discover(PortModule module, TestWorld world, PortDomain<Result> domain) {
        List<Result> results = new ArrayList<>();
        module.discovery().discover(
                world, 0, 0, 0, 0, domain,
                (x, y, z, portId, result, sourceFirst) -> results.add(result)
        );
        return results;
    }

    private static final class TestWorld implements PortWorldView {
        private final PortModule module;

        private final Map<Position, Integer> blocks = new HashMap<>();

        TestWorld(PortModule module) {
            this.module = module;
        }

        void put(int x, int y, int z, int blockTypeId) {
            blocks.put(new Position(x, y, z), blockTypeId);
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
