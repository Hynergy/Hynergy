package dev.hynergy.core.port;

import com.hypixel.hytale.server.core.asset.type.blocktype.config.RotationTuple;
import org.jspecify.annotations.Nullable;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.*;

final class PortDiscoveryTest {
    private record Profile(String name) {
    }

    private record OtherProfile(String name) {
    }

    private record Result(String value) {
    }

    @Test
    void worldSelectedDefinitionsAreDiscoveredWithoutBlockTypeRegistration() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result(first.name() + ":" + second.name())
        );
        PortModuleTestAccess.freeze(module);
        BlockPortDefinition source = BlockPortDefinition.of(port(0, 1, standard, new Profile("source")));
        BlockPortDefinition target = BlockPortDefinition.of(port(5, -1, standard, new Profile("target")));
        PortWorldView world = new TestWorld(module) {
            @Override
            public @Nullable BlockPortDefinition portsAt(int x, int y, int z) {
                if (y != 0 || z != 0) {
                    return null;
                }
                return switch (x) {
                    case 0 -> source;
                    case 1 -> target;
                    default -> null;
                };
            }
        };

        List<Match> matches = discover(module.discovery(), world, 0, 0, 0, 0, domain);

        assertEquals(List.of(new Match(5, new Result("source:target"), true)), matches);
        assertTrue(discover(module.discovery(), world, 2, 0, 0, 0, domain).isEmpty());
    }

    @Test
    void sameStandardConnectionIsDiscovered() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result(first.name() + ":" + second.name())
        );
        PortModuleTestAccess.freeze(module);
        installPair(module, standard, new Profile("source"), new Profile("target"));
        TestWorld world = pairWorld(module);

        List<Match> matches = discover(module.discovery(), world, 0, 0, 0, 0, domain);

        assertEquals(1, matches.size());
        assertEquals(5, matches.getFirst().targetPortId());
        assertEquals(new Result("source:target"), matches.getFirst().result());
        assertTrue(matches.getFirst().sourceIsResolverFirst());
    }

    @Test
    void incompatibleStandardWithoutAdapterIsIgnored() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> first = module.registerStandard(
                "test:first", domain, Profile.class,
                (a, b, geometry) -> new Result("first")
        );
        PortStandard<OtherProfile, Result> second = module.registerStandard(
                "test:second", domain, OtherProfile.class,
                (a, b, geometry) -> new Result("second")
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(port(0, 1, first, new Profile("source"))));
        module.setBlockPorts(2, BlockPortDefinition.of(port(5, -1, second, new OtherProfile("target"))));

        assertTrue(discover(module.discovery(), pairWorld(module), 0, 0, 0, 0, domain).isEmpty());
    }

    @Test
    void adapterRegisteredByNewStandardConnectsToExistingStandardInEitherDirection() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<OtherProfile, Result> existing = module.registerStandard(
                "test:existing", domain, OtherProfile.class,
                (a, b, geometry) -> new Result("existing")
        );
        PortStandard<Profile, Result> plugin = module.registerStandard(
                "plugin:new", domain, Profile.class,
                (a, b, geometry) -> new Result("plugin")
        );
        AtomicInteger calls = new AtomicInteger();
        module.registerAdapter(
                plugin,
                existing,
                (pluginProfile, existingProfile, geometry) -> {
                    calls.incrementAndGet();
                    return new Result(pluginProfile.name() + "->" + existingProfile.name());
                }
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(port(3, -1, existing, new OtherProfile("existing"))));
        module.setBlockPorts(2, BlockPortDefinition.of(port(7, 1, plugin, new Profile("plugin"))));
        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 2);
        world.put(1, 0, 0, 1);

        List<Match> forward = discover(module.discovery(), world, 0, 0, 0, 7, domain);
        List<Match> reverse = discover(module.discovery(), world, 1, 0, 0, 3, domain);

        assertEquals(new Result("plugin->existing"), forward.getFirst().result());
        assertTrue(forward.getFirst().sourceIsResolverFirst());
        assertEquals(new Result("plugin->existing"), reverse.getFirst().result());
        assertFalse(reverse.getFirst().sourceIsResolverFirst());
        assertEquals(2, calls.get());
    }


    @Test
    void candidateDefinitionIsReadOnceWhenSourceHasMultipleCompatibleRules() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> source = module.registerStandard(
                "test:source", domain, Profile.class,
                (first, second, geometry) -> new Result("self")
        );
        PortStandard<OtherProfile, Result> firstTarget = module.registerStandard(
                "test:first-target", domain, OtherProfile.class,
                (first, second, geometry) -> new Result("first-self")
        );
        PortStandard<OtherProfile, Result> secondTarget = module.registerStandard(
                "test:second-target", domain, OtherProfile.class,
                (first, second, geometry) -> new Result("second-self")
        );
        module.registerAdapter(
                source, firstTarget,
                (sourceProfile, targetProfile, geometry) -> new Result("first")
        );
        module.registerAdapter(
                source, secondTarget,
                (sourceProfile, targetProfile, geometry) -> new Result("second")
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(
                port(0, 1, source, new Profile("source"))
        ));
        module.setBlockPorts(2, BlockPortDefinition.of(
                port(4, -1, firstTarget, new OtherProfile("first")),
                port(5, -1, secondTarget, new OtherProfile("second"))
        ));
        CountingWorld world = new CountingWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);

        List<Match> matches = discover(module.discovery(), world, 0, 0, 0, 0, domain);

        assertEquals(2, matches.size());
        assertEquals(List.of(4, 5), matches.stream().map(Match::targetPortId).sorted().toList());
        assertEquals(1, world.definitionReads(0, 0, 0));
        assertEquals(1, world.definitionReads(1, 0, 0));
        assertEquals(1, world.rotationReads(0, 0, 0));
        assertEquals(1, world.rotationReads(1, 0, 0));
    }

    @Test
    void resolverRejectionDoesNotStopLaterCandidateOffset() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> second.name().equals("accept") ? new Result("ok") : null
        );
        PortModuleTestAccess.freeze(module);
        PortReach twoCandidates = PortReach.of(new PortOffset(1, 0, 0), new PortOffset(2, 0, 0));
        module.setBlockPorts(1, BlockPortDefinition.of(
                new PortDefinition<>(0, PortOffset.ZERO, twoCandidates, standard, new Profile("source"))
        ));
        module.setBlockPorts(2, BlockPortDefinition.of(port(1, -1, standard, new Profile("reject"))));
        module.setBlockPorts(3, BlockPortDefinition.of(
                new PortDefinition<>(2, PortOffset.ZERO, PortReach.single(-2, 0, 0), standard, new Profile("accept"))
        ));
        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);
        world.put(2, 0, 0, 3);

        List<Match> matches = discover(module.discovery(), world, 0, 0, 0, 0, domain);
        assertEquals(1, matches.size());
        assertEquals(2, matches.getFirst().targetPortId());
    }

    @Test
    void unavailableCandidateBlockIsSkippedBeforeResolution() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        AtomicInteger calls = new AtomicInteger();
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> {
                    calls.incrementAndGet();
                    return new Result("ok");
                }
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(port(0, 1, standard, new Profile("source"))));
        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);

        assertTrue(discover(module.discovery(), world, 0, 0, 0, 0, domain).isEmpty());
        assertEquals(0, calls.get());
    }

    @Test
    void sourceDomainMismatchIsRejected() {
        PortModule module = new PortModule();
        PortDomain<Result> firstDomain = module.registerDomain("test:first");
        PortDomain<Result> secondDomain = module.registerDomain("test:second");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", firstDomain, Profile.class,
                (first, second, geometry) -> new Result("ok")
        );
        PortModuleTestAccess.freeze(module);
        module.setBlockPorts(1, BlockPortDefinition.of(port(0, 1, standard, new Profile("source"))));
        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);

        assertThrows(
                IllegalArgumentException.class,
                () -> module.discovery().discover(world, 0, 0, 0, 0, secondDomain, (a, b, c, d, e, f) -> {
                })
        );
    }

    @Test
    void discoveryIsConcurrentAcrossThreadsButNotReentrantOnOneThread() throws Exception {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> new Result("ok")
        );
        PortModuleTestAccess.freeze(module);
        installPair(module, standard, new Profile("source"), new Profile("target"));
        TestWorld world = pairWorld(module);

        CountDownLatch entered = new CountDownLatch(2);
        CountDownLatch release = new CountDownLatch(1);
        ExecutorService executor = Executors.newFixedThreadPool(2);
        try {
            Future<?> first = executor.submit(() -> module.discovery().discover(
                    world, 0, 0, 0, 0, domain,
                    (x, y, z, port, result, sourceFirst) -> awaitTogether(entered, release)
            ));
            Future<?> second = executor.submit(() -> module.discovery().discover(
                    world, 0, 0, 0, 0, domain,
                    (x, y, z, port, result, sourceFirst) -> awaitTogether(entered, release)
            ));
            assertTrue(entered.await(2, TimeUnit.SECONDS));
            release.countDown();
            first.get(2, TimeUnit.SECONDS);
            second.get(2, TimeUnit.SECONDS);
        } finally {
            release.countDown();
            executor.shutdownNow();
        }

        assertThrows(
                IllegalStateException.class,
                () -> module.discovery().discover(
                        world, 0, 0, 0, 0, domain,
                        (x, y, z, port, result, sourceFirst) -> module.discovery().discover(
                                world, 0, 0, 0, 0, domain, (a, b, c, d, e, f) -> {
                                }
                        )
                )
        );
    }

    private static void awaitTogether(CountDownLatch entered, CountDownLatch release) {
        entered.countDown();
        try {
            if (!release.await(2, TimeUnit.SECONDS)) {
                throw new AssertionError("Timed out waiting for concurrent discovery");
            }
        } catch (InterruptedException exception) {
            Thread.currentThread().interrupt();
            throw new AssertionError(exception);
        }
    }

    private static <P, R> PortDefinition<P, R> port(
            int id,
            int dx,
            PortStandard<P, R> standard,
            P profile
    ) {
        return new PortDefinition<>(id, PortOffset.ZERO, PortReach.single(dx, 0, 0), standard, profile);
    }

    private static <P, R> void installPair(
            PortModule module,
            PortStandard<P, R> standard,
            P source,
            P target
    ) {
        module.setBlockPorts(1, BlockPortDefinition.of(port(0, 1, standard, source)));
        module.setBlockPorts(2, BlockPortDefinition.of(port(5, -1, standard, target)));
    }

    private static TestWorld pairWorld(PortModule module) {
        TestWorld world = new TestWorld(module);
        world.put(0, 0, 0, 1);
        world.put(1, 0, 0, 2);
        return world;
    }

    private static <R> List<Match> discover(
            PortDiscovery discovery,
            PortWorldView world,
            int x,
            int y,
            int z,
            int port,
            PortDomain<R> domain
    ) {
        List<Match> matches = new ArrayList<>();
        discovery.discover(
                world, x, y, z, port, domain,
                (targetX, targetY, targetZ, targetPort, result, sourceFirst) ->
                        matches.add(new Match(targetPort, result, sourceFirst))
        );
        return matches;
    }

    private record Match(int targetPortId, Object result, boolean sourceIsResolverFirst) {
    }

    private static class TestWorld implements PortWorldView {
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


    private static final class CountingWorld extends TestWorld {
        private final Map<Position, Integer> definitionReads = new HashMap<>();
        private final Map<Position, Integer> rotationReads = new HashMap<>();

        CountingWorld(PortModule module) {
            super(module);
        }

        @Override
        public @Nullable BlockPortDefinition portsAt(int x, int y, int z) {
            Position position = new Position(x, y, z);
            definitionReads.merge(position, 1, Integer::sum);
            return super.portsAt(x, y, z);
        }

        @Override
        public RotationTuple rotation(int x, int y, int z) {
            Position position = new Position(x, y, z);
            rotationReads.merge(position, 1, Integer::sum);
            return super.rotation(x, y, z);
        }

        int definitionReads(int x, int y, int z) {
            return definitionReads.getOrDefault(new Position(x, y, z), 0);
        }

        int rotationReads(int x, int y, int z) {
            return rotationReads.getOrDefault(new Position(x, y, z), 0);
        }
    }

    private record Position(int x, int y, int z) {
    }
}
