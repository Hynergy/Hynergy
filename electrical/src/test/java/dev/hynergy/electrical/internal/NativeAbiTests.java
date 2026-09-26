package dev.hynergy.electrical.internal;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

public class NativeAbiTests
{
    @Test
    void nativeAbiIsCompatible()
    {
        assertEquals(5, NativeBindings.abiVersion());
        assertTrue(NativeBindings.abiRevision() >= 0);
    }
}
