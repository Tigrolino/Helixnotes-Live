# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# Preserve JS interface annotations
-keepattributes JavascriptInterface

# Keep our custom WebView JS bridge and all its public methods
-keep class com.helixnotes.app.StorageBridge {
   public *;
}

# Keep MainActivity public methods (called from StorageBridge)
-keep class com.helixnotes.app.MainActivity {
   public *;
}

# Ignore missing java.beans classes (referenced by Jackson but not available on Android)
-dontwarn java.beans.**
