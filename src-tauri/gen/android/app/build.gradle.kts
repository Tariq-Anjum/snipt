import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing (S16). Secrets live outside git: SNIPT_KEYSTORE_PROPS or
// ~/.config/mysms-signing/keystore.properties (storeFile, storePassword, keyAlias, keyPassword).
val keystorePropsFile = File(
    System.getenv("SNIPT_KEYSTORE_PROPS")
        ?: "${System.getProperty("user.home")}/.config/mysms-signing/keystore.properties"
)
val releaseRequested = gradle.startParameter.taskNames.any { it.contains("release", ignoreCase = true) }
val keystoreProps = Properties()
if (keystorePropsFile.isFile) {
    keystorePropsFile.inputStream().use { keystoreProps.load(it) }
} else if (releaseRequested) {
    throw GradleException(
        "Snipt release signing: keystore properties not found at ${keystorePropsFile.path}. " +
            "Run scripts/make-keystore.sh or set SNIPT_KEYSTORE_PROPS."
    )
}
// Release ABI: arm64-v8a (owner's phone) unless SNIPT_ABI overrides (e.g. x86_64 for emulator smoke).
val sniptAbi = System.getenv("SNIPT_ABI") ?: "arm64-v8a"

android {
    compileSdk = 37
    ndkVersion = "29.0.14206865"
    namespace = "app.snipt.sms"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "app.snipt.sms"
        minSdk = 29
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (keystoreProps.containsKey("storeFile")) {
            create("release") {
                storeFile = file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfigs.findByName("release")?.let { signingConfig = it }
            ndk { abiFilters.clear(); abiFilters.add(sniptAbi) }
            isMinifyEnabled = true
            isShrinkResources = true
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
