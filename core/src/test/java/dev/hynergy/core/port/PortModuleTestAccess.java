package dev.hynergy.core.port;

public final class PortModuleTestAccess {
    private PortModuleTestAccess() {
    }

    public static void freeze(PortModule module) {
        module.freezeForTest();
    }
}
