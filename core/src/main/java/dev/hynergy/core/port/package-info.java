/**
 * Defines ports and discovers compatible port pairs.
 *
 * <p>A world view selects the port layout at each position. Discovery applies
 * block rotations and checks whether each port can reach the other block.
 * A registered resolver evaluates each compatible pair and produces a result.</p>
 *
 * <p>The caller owns connection state. The caller must check stored results
 * again after a relevant change to the world or a port layout.</p>
 */
package dev.hynergy.core.port;
