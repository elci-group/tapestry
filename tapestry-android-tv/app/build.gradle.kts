plugins {
    id("com.android.application")
}

android {
    namespace = "org.tapestry.androidtv"
    compileSdk = 34

    defaultConfig {
        applicationId = "org.tapestry.androidtv"
        minSdk = 23
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }
}
