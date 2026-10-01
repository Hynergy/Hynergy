package dev.hynergy.core.port;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

final class HytalePortWorldViewTest {

    @Test
    void converts_world_coordinates_to_section_local_coordinates() {
        assertEquals(0, HytalePortWorldView.localCoordinate(0));
        assertEquals(31, HytalePortWorldView.localCoordinate(31));
        assertEquals(0, HytalePortWorldView.localCoordinate(32));
        assertEquals(1, HytalePortWorldView.localCoordinate(33));
        assertEquals(31, HytalePortWorldView.localCoordinate(-1));
        assertEquals(0, HytalePortWorldView.localCoordinate(-32));
        assertEquals(31, HytalePortWorldView.localCoordinate(-33));
    }

    @Test
    void missing_loaded_section_is_unavailable_not_air() {
        assertEquals(-1, HytalePortWorldView.blockTypeIdFromSection(null, 0, 0, 0));
    }
}
