package dev.hynergy.electrical.internal;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

public class NativeLayoutTest {
    @Test
    public void tickResultLayoutFixture() {
        assertEquals(0, NativeLayouts.TICK_RESULT_RECORD_COUNT_OFFSET);
        assertEquals(4, NativeLayouts.TICK_RESULT_REQUIRED_CAPACITY_OFFSET);
        assertEquals(8, NativeLayouts.TICK_RESULT_DEVICE_ID_OFFSET);
        assertEquals(12, NativeLayouts.TICK_RESULT_PARAMETER_ID_OFFSET);
        assertEquals(16, NativeLayouts.TICK_RESULT_ITERATIONS_OFFSET);
    }
}
