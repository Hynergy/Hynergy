package dev.hynergy.electrical.internal;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

public class NativeLayoutTest {
    @Test
    void nativeAbiIsCompatible() {
        assertEquals(3, NativeBindings.abiVersion());
        assertTrue(NativeBindings.abiRevision() >= 0);
    }
}
