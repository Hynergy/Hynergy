package dev.hynergy.core.port;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

final class PortRegistryTest {
    private record ProfileA(int value) {}
    private record ProfileB(int value) {}
    private enum Result { CONNECTED }

    private static final PortResolver<ProfileA, ProfileA, Result> RESOLVER_A =
        (first, second, geometry) -> Result.CONNECTED;
    private static final PortResolver<ProfileB, ProfileB, Result> RESOLVER_B =
        (first, second, geometry) -> Result.CONNECTED;

    @Test
    void duplicateDomainIdIsRejected() {
        PortRegistry registry = new PortRegistry();
        registry.<Result>registerDomain("test:mechanical");
        assertThrows(IllegalStateException.class, () -> registry.<Result>registerDomain("test:mechanical"));
    }

    @Test
    void duplicateStandardIdIsRejected() {
        PortRegistry registry = new PortRegistry();
        PortDomain<Result> domain = registry.registerDomain("test:mechanical");
        registry.registerStandard("test:a", domain, ProfileA.class, RESOLVER_A);
        assertThrows(
            IllegalStateException.class,
            () -> registry.registerStandard("test:a", domain, ProfileA.class, RESOLVER_A)
        );
    }

    @Test
    void adapterAcrossDifferentDomainsIsRejected() {
        PortRegistry registry = new PortRegistry();
        PortDomain<Result> firstDomain = registry.registerDomain("test:first");
        PortDomain<Result> secondDomain = registry.registerDomain("test:second");
        PortStandard<ProfileA, Result> first =
            registry.registerStandard("test:a", firstDomain, ProfileA.class, RESOLVER_A);
        PortStandard<ProfileB, Result> second =
            registry.registerStandard("test:b", secondDomain, ProfileB.class, RESOLVER_B);

        assertThrows(
            IllegalArgumentException.class,
            () -> registry.registerAdapter(first, second, (a, b, geometry) -> Result.CONNECTED)
        );
    }

    @Test
    void duplicateRuleForSameStandardPairIsRejectedRegardlessOfRegistrationOrder() {
        PortRegistry registry = new PortRegistry();
        PortDomain<Result> domain = registry.registerDomain("test:mechanical");
        PortStandard<ProfileA, Result> first =
            registry.registerStandard("test:a", domain, ProfileA.class, RESOLVER_A);
        PortStandard<ProfileB, Result> second =
            registry.registerStandard("test:b", domain, ProfileB.class, RESOLVER_B);

        registry.registerAdapter(first, second, (a, b, geometry) -> Result.CONNECTED);
        assertThrows(
            IllegalStateException.class,
            () -> registry.registerAdapter(second, first, (b, a, geometry) -> Result.CONNECTED)
        );
    }


    @Test
    void frozenRuleLookupPreservesCanonicalDirectionAndIdentity() {
        PortRegistry registry = new PortRegistry();
        PortDomain<Result> domain = registry.registerDomain("test:mechanical");
        PortStandard<ProfileA, Result> first =
            registry.registerStandard("test:a", domain, ProfileA.class, RESOLVER_A);
        PortStandard<ProfileB, Result> second =
            registry.registerStandard("test:b", domain, ProfileB.class, RESOLVER_B);
        PortStandard<ProfileA, Result> unrelated =
            registry.registerStandard("test:unrelated", domain, ProfileA.class, RESOLVER_A);
        registry.registerAdapter(first, second, (a, b, geometry) -> Result.CONNECTED);
        registry.freeze();

        PortRegistry.RuleSide<?, ?, ?> same = registry.ruleSide(first, first);
        PortRegistry.RuleSide<?, ?, ?> forward = registry.ruleSide(first, second);
        PortRegistry.RuleSide<?, ?, ?> reverse = registry.ruleSide(second, first);

        assertNotNull(same);
        assertTrue(same.sourceIsFirst());
        assertNotNull(forward);
        assertTrue(forward.sourceIsFirst());
        assertNotNull(reverse);
        assertFalse(reverse.sourceIsFirst());
        assertSame(forward.rule(), reverse.rule());
        assertNull(registry.ruleSide(first, unrelated));

        PortRegistry foreign = new PortRegistry();
        PortDomain<Result> foreignDomain = foreign.registerDomain("test:foreign");
        PortStandard<ProfileA, Result> foreignFirst =
            foreign.registerStandard("test:a", foreignDomain, ProfileA.class, RESOLVER_A);
        assertNull(registry.ruleSide(first, foreignFirst));
    }

    @Test
    void blankIdsAreRejected() {
        PortRegistry registry = new PortRegistry();
        assertThrows(IllegalArgumentException.class, () -> registry.<Result>registerDomain("  "));
        PortDomain<Result> domain = registry.registerDomain("test:mechanical");
        assertThrows(
            IllegalArgumentException.class,
            () -> registry.registerStandard("", domain, ProfileA.class, RESOLVER_A)
        );
    }
}
