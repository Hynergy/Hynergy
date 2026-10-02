package dev.hynergy.core.port;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class PortModuleTest {
    private record Profile(int value) {
    }

    private enum Result {CONNECTED}

    @Test
    void protocolRegistrationIsOpenBeforeStartAndFrozenAfterward() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> Result.CONNECTED
        );
        assertEquals(standard, module.standard("test:standard"));

        PortModuleTestAccess.freeze(module);
        assertThrows(IllegalStateException.class, () -> module.registerDomain("test:late"));
    }

    @Test
    void runtimeBlockDefinitionsRemainMutableAfterProtocolFreeze() {
        PortModule module = new PortModule();
        PortDomain<Result> domain = module.registerDomain("test:domain");
        PortStandard<Profile, Result> standard = module.registerStandard(
                "test:standard", domain, Profile.class,
                (first, second, geometry) -> Result.CONNECTED
        );
        PortModuleTestAccess.freeze(module);

        BlockPortDefinition definition = BlockPortDefinition.of(
                new PortDefinition<>(0, PortOffset.ZERO, PortReach.single(1, 0, 0), standard, new Profile(1))
        );
        module.setBlockPorts(42, definition);
        assertEquals(definition, module.blockPorts(42));
        module.clearBlockPorts(42);
        assertNull(module.blockPorts(42));
    }

    @Test
    void discoveryBeforeStartIsRejected() {
        PortModule module = new PortModule();
        assertThrows(IllegalStateException.class, module::discovery);
    }

    @Test
    void freezeMakesDiscoveryAvailable() {
        PortModule module = new PortModule();
        PortModuleTestAccess.freeze(module);
        assertNotNull(module.discovery());
    }
}
