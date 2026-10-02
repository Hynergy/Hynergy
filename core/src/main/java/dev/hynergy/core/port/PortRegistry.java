package dev.hynergy.core.port;

import it.unimi.dsi.fastutil.objects.Object2ObjectLinkedOpenHashMap;

import java.util.IdentityHashMap;
import java.util.Map;
import java.util.Objects;

final class PortRegistry {
    private final Map<String, PortDomain<?>> domains = new Object2ObjectLinkedOpenHashMap<>();
    private final Map<String, PortStandard<?, ?>> standards = new Object2ObjectLinkedOpenHashMap<>();
    private final Map<StandardPair, Rule<?, ?, ?>> rules = new Object2ObjectLinkedOpenHashMap<>();

    private Map<PortStandard<?, ?>, Map<PortStandard<?, ?>, RuleSide<?, ?, ?>>> ruleSidesByPair;
    private boolean frozen;

    <R> PortDomain<R> registerDomain(String id) {
        requireOpen();
        requireValidId(id, "domain id");

        if (domains.containsKey(id)) {
            throw new IllegalStateException("Port domain is already registered: " + id);
        }

        PortDomain<R> domain = new PortDomain<>(id);
        domains.put(id, domain);
        return domain;
    }

    <P, R> PortStandard<P, R> registerStandard(
            String id,
            PortDomain<R> domain,
            Class<P> profileType,
            PortResolver<P, P, R> resolver
    ) {
        requireOpen();
        requireValidId(id, "standard id");
        requireRegisteredDomain(domain);
        Objects.requireNonNull(profileType, "profileType");
        Objects.requireNonNull(resolver, "resolver");

        if (standards.containsKey(id)) {
            throw new IllegalStateException("Port standard is already registered: " + id);
        }

        PortStandard<P, R> standard = new PortStandard<>(id, domain, profileType);
        standards.put(id, standard);
        rules.put(new StandardPair(standard, standard), new Rule<>(standard, standard, resolver));
        return standard;
    }

    <A, B, R> void registerAdapter(
            PortStandard<A, R> first,
            PortStandard<B, R> second,
            PortResolver<A, B, R> resolver
    ) {
        requireOpen();
        requireRegisteredStandard(first);
        requireRegisteredStandard(second);
        Objects.requireNonNull(resolver, "resolver");

        if (first.domain() != second.domain()) {
            throw new IllegalArgumentException("Port adapters must stay within one domain");
        }

        StandardPair pair = new StandardPair(first, second);
        if (rules.containsKey(pair)) {
            throw new IllegalStateException(
                    "A port connection rule is already registered for " + first.id() + " and " + second.id()
            );
        }

        rules.put(pair, new Rule<>(first, second, resolver));
    }

    void freeze() {
        requireOpen();
        ruleSidesByPair = compileRuleSides();
        frozen = true;
    }

    private Map<PortStandard<?, ?>, Map<PortStandard<?, ?>, RuleSide<?, ?, ?>>> compileRuleSides() {
        IdentityHashMap<PortStandard<?, ?>, Map<PortStandard<?, ?>, RuleSide<?, ?, ?>>> compiled =
                new IdentityHashMap<>();

        for (Rule<?, ?, ?> rule : rules.values()) {
            putRuleSide(compiled, rule.first(), rule.second(), new RuleSide<>(rule, true));

            if (rule.first() != rule.second()) {
                putRuleSide(compiled, rule.second(), rule.first(), new RuleSide<>(rule, false));
            }
        }

        return compiled;
    }

    private static void putRuleSide(
            IdentityHashMap<PortStandard<?, ?>, Map<PortStandard<?, ?>, RuleSide<?, ?, ?>>> compiled,
            PortStandard<?, ?> source,
            PortStandard<?, ?> target,
            RuleSide<?, ?, ?> side
    ) {
        compiled.computeIfAbsent(source, ignored -> new IdentityHashMap<>())
                .put(target, side);
    }

    RuleSide<?, ?, ?> ruleSide(PortStandard<?, ?> source, PortStandard<?, ?> target) {
        if (!frozen) {
            throw new IllegalStateException("Port registry must be frozen before discovery");
        }

        Map<PortStandard<?, ?>, RuleSide<?, ?, ?>> targets = ruleSidesByPair.get(source);
        return targets == null ? null : targets.get(target);
    }

    PortStandard<?, ?> standard(String id) {
        Objects.requireNonNull(id, "id");
        return standards.get(id);
    }

    boolean owns(PortStandard<?, ?> standard) {
        Objects.requireNonNull(standard, "standard");
        return standards.get(standard.id()) == standard;
    }

    boolean isFrozen() {
        return frozen;
    }

    private void requireOpen() {
        if (frozen) {
            throw new IllegalStateException("Port registry is frozen");
        }
    }

    private void requireRegisteredDomain(PortDomain<?> domain) {
        Objects.requireNonNull(domain, "domain");
        if (domains.get(domain.id()) != domain) {
            throw new IllegalArgumentException("Port domain is not registered in this registry: " + domain.id());
        }
    }

    private void requireRegisteredStandard(PortStandard<?, ?> standard) {
        Objects.requireNonNull(standard, "standard");
        if (standards.get(standard.id()) != standard) {
            throw new IllegalArgumentException("Port standard is not registered in this registry: " + standard.id());
        }
    }

    private static void requireValidId(String id, String name) {
        Objects.requireNonNull(id, name);
        if (id.isBlank()) {
            throw new IllegalArgumentException(name + " must not be blank");
        }
    }

    record Rule<A, B, R>(PortStandard<A, R> first, PortStandard<B, R> second, PortResolver<A, B, R> resolver) {
    }

    record RuleSide<A, B, R>(Rule<A, B, R> rule, boolean sourceIsFirst) {
    }

    private static final class StandardPair {
        private final PortStandard<?, ?> low;
        private final PortStandard<?, ?> high;

        StandardPair(PortStandard<?, ?> first, PortStandard<?, ?> second) {
            if (compare(first, second) <= 0) {
                low = first;
                high = second;
            } else {
                low = second;
                high = first;
            }
        }

        private static int compare(PortStandard<?, ?> first, PortStandard<?, ?> second) {
            int byId = first.id().compareTo(second.id());
            if (byId != 0) {
                return byId;
            }
            return Integer.compare(System.identityHashCode(first), System.identityHashCode(second));
        }

        @Override
        public boolean equals(Object object) {
            return object instanceof StandardPair other && low == other.low && high == other.high;
        }

        @Override
        public int hashCode() {
            return System.identityHashCode(low) * 31 + System.identityHashCode(high);
        }
    }
}
