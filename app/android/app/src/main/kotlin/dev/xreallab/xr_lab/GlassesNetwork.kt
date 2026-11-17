package dev.xreallab.xr_lab

import android.content.Context
import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.util.Log
import java.net.Inet4Address

/**
 * Acha a rede USB do óculos (Ethernet sem internet, endereços 169.254.x) e avisa o handle
 * (`Network.getNetworkHandle()`) sempre que ela aparece, muda ou some (`null`).
 * O núcleo Rust usa o handle para ligar os sockets a essa rede (android_setsocknetwork).
 */
class GlassesNetwork(context: Context, private val onChange: (Long?) -> Unit) {
    private val cm = context.getSystemService(ConnectivityManager::class.java)
    private val links = LinkedHashMap<Network, LinkProperties?>()
    private var current: Long? = null

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            synchronized(links) { links[network] = cm.getLinkProperties(network) }
            publish()
        }

        override fun onLinkPropertiesChanged(network: Network, lp: LinkProperties) {
            synchronized(links) { links[network] = lp }
            publish()
        }

        override fun onLost(network: Network) {
            synchronized(links) { links.remove(network) }
            publish()
        }
    }

    fun start() {
        val b = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED)
        if (Build.VERSION.SDK_INT >= 31) b.addTransportType(NetworkCapabilities.TRANSPORT_USB)
        cm.registerNetworkCallback(b.build(), callback)
    }

    fun stop() {
        runCatching { cm.unregisterNetworkCallback(callback) }
    }

    /** Prefere a rede com endereço 169.254.2.x (NCM, a que o Nebula usa); senão 169.254.1.x. */
    private fun score(lp: LinkProperties?): Int {
        val addrs = lp?.linkAddresses?.mapNotNull { it.address as? Inet4Address } ?: return 0
        return when {
            addrs.any { it.hostAddress?.startsWith("169.254.2.") == true } -> 2
            addrs.any { it.hostAddress?.startsWith("169.254.1.") == true } -> 1
            else -> 0
        }
    }

    private fun publish() {
        val best = synchronized(links) {
            links.entries.filter { score(it.value) > 0 }.maxByOrNull { score(it.value) }
        }
        synchronized(links) {
            for ((n, lp) in links) {
                val caps = cm.getNetworkCapabilities(n)
                Log.i(TAG, "rede ${n.networkHandle}: ${lp?.interfaceName} ${lp?.linkAddresses} internet=${caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)} restrita=${caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED) == false}")
            }
        }
        val handle = best?.key?.networkHandle
        if (handle != current) {
            current = handle
            Log.i(TAG, "rede do óculos: ${best?.value?.interfaceName ?: "nenhuma"} (handle $handle)")
            onChange(handle)
        }
    }

    companion object {
        const val TAG = "xr-lab"
    }
}
