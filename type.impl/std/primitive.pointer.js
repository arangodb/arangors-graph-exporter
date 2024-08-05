(function() {var type_impls = {
"core_foundation":[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-FromMutVoid-for-*const+c_void\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#319-323\">source</a><a href=\"#impl-FromMutVoid-for-*const+c_void\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"core_foundation/base/trait.FromMutVoid.html\" title=\"trait core_foundation::base::FromMutVoid\">FromMutVoid</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.from_mut_void\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#320-322\">source</a><a href=\"#method.from_mut_void\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation/base/trait.FromMutVoid.html#tymethod.from_mut_void\" class=\"fn\">from_mut_void</a>&lt;'a&gt;(x: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*mut </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a>) -&gt; <a class=\"struct\" href=\"core_foundation/base/struct.ItemMutRef.html\" title=\"struct core_foundation::base::ItemMutRef\">ItemMutRef</a>&lt;'a, Self&gt;</h4></section></div></details>","FromMutVoid","core_foundation::mach_port::CFAllocatorRef","core_foundation::base::CFNullRef","core_foundation::base::CFTypeRef","core_foundation::propertylist::CFPropertyListRef"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-FromVoid-for-*const+c_void\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#349-353\">source</a><a href=\"#impl-FromVoid-for-*const+c_void\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"core_foundation/base/trait.FromVoid.html\" title=\"trait core_foundation::base::FromVoid\">FromVoid</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.from_void\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#350-352\">source</a><a href=\"#method.from_void\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation/base/trait.FromVoid.html#tymethod.from_void\" class=\"fn\">from_void</a>&lt;'a&gt;(x: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a>) -&gt; <a class=\"struct\" href=\"core_foundation/base/struct.ItemRef.html\" title=\"struct core_foundation::base::ItemRef\">ItemRef</a>&lt;'a, Self&gt;</h4></section></div></details>","FromVoid","core_foundation::mach_port::CFAllocatorRef","core_foundation::base::CFNullRef","core_foundation::base::CFTypeRef","core_foundation::propertylist::CFPropertyListRef"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-TCFTypeRef-for-*const+T\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#112\">source</a><a href=\"#impl-TCFTypeRef-for-*const+T\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;T&gt; <a class=\"trait\" href=\"core_foundation/base/trait.TCFTypeRef.html\" title=\"trait core_foundation::base::TCFTypeRef\">TCFTypeRef</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const T</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.as_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#113\">source</a><a href=\"#method.as_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"core_foundation/base/trait.TCFTypeRef.html#tymethod.as_void_ptr\" class=\"fn\">as_void_ptr</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"enum\" href=\"https://doc.rust-lang.org/1.79.0/core/ffi/enum.c_void.html\" title=\"enum core::ffi::c_void\">c_void</a></h4></section><section id=\"method.from_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#117\">source</a><a href=\"#method.from_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation/base/trait.TCFTypeRef.html#tymethod.from_void_ptr\" class=\"fn\">from_void_ptr</a>(ptr: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"enum\" href=\"https://doc.rust-lang.org/1.79.0/core/ffi/enum.c_void.html\" title=\"enum core::ffi::c_void\">c_void</a>) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const T</a></h4></section></div></details>","TCFTypeRef","core_foundation::array::CFArrayRef","core_foundation::attributed_string::CFAttributedStringRef","core_foundation::mach_port::CFAllocatorRef","core_foundation::base::CFNullRef","core_foundation::base::CFTypeRef","core_foundation::base::ConstStr255Param","core_foundation::base::ConstStringPtr","core_foundation::number::CFBooleanRef","core_foundation::characterset::CFCharacterSetRef","core_foundation::data::CFDataRef","core_foundation::date::CFDateRef","core_foundation::dictionary::CFDictionaryRef","core_foundation::error::CFErrorDomain","core_foundation::number::CFNumberRef","core_foundation::propertylist::CFPropertyListRef","core_foundation::set::CFSetRef","core_foundation::string::CFStringRef","core_foundation::timezone::CFTimeZoneRef","core_foundation::url::CFURLRef","core_foundation::uuid::CFUUIDRef"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-TCFTypeRef-for-*mut+T\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#122\">source</a><a href=\"#impl-TCFTypeRef-for-*mut+T\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;T&gt; <a class=\"trait\" href=\"core_foundation/base/trait.TCFTypeRef.html\" title=\"trait core_foundation::base::TCFTypeRef\">TCFTypeRef</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*mut T</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.as_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#123\">source</a><a href=\"#method.as_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"core_foundation/base/trait.TCFTypeRef.html#tymethod.as_void_ptr\" class=\"fn\">as_void_ptr</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"enum\" href=\"https://doc.rust-lang.org/1.79.0/core/ffi/enum.c_void.html\" title=\"enum core::ffi::c_void\">c_void</a></h4></section><section id=\"method.from_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#127\">source</a><a href=\"#method.from_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation/base/trait.TCFTypeRef.html#tymethod.from_void_ptr\" class=\"fn\">from_void_ptr</a>(ptr: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"enum\" href=\"https://doc.rust-lang.org/1.79.0/core/ffi/enum.c_void.html\" title=\"enum core::ffi::c_void\">c_void</a>) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*mut T</a></h4></section></div></details>","TCFTypeRef","core_foundation::array::CFMutableArrayRef","core_foundation::attributed_string::CFMutableAttributedStringRef","core_foundation::base::StringPtr","core_foundation::bundle::CFBundleRef","core_foundation::bundle::CFPlugInRef","core_foundation::characterset::CFMutableCharacterSetRef","core_foundation::data::CFMutableDataRef","core_foundation::dictionary::CFMutableDictionaryRef","core_foundation::error::CFErrorRef","core_foundation::filedescriptor::CFFileDescriptorRef","core_foundation::mach_port::CFMachPortRef","core_foundation::runloop::CFRunLoopRef","core_foundation::runloop::CFRunLoopSourceRef","core_foundation::runloop::CFRunLoopObserverRef","core_foundation::runloop::CFRunLoopTimerRef","core_foundation::set::CFMutableSetRef","core_foundation::string::CFMutableStringRef"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-ToVoid%3C*const+c_void%3E-for-*const+c_void\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#369-373\">source</a><a href=\"#impl-ToVoid%3C*const+c_void%3E-for-*const+c_void\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"core_foundation/base/trait.ToVoid.html\" title=\"trait core_foundation::base::ToVoid\">ToVoid</a>&lt;<a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"enum\" href=\"https://doc.rust-lang.org/1.79.0/core/ffi/enum.c_void.html\" title=\"enum core::ffi::c_void\">c_void</a>&gt; for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.to_void\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation/base.rs.html#370-372\">source</a><a href=\"#method.to_void\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"core_foundation/base/trait.ToVoid.html#tymethod.to_void\" class=\"fn\">to_void</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h4></section></div></details>","ToVoid<*const c_void>","core_foundation::mach_port::CFAllocatorRef","core_foundation::base::CFNullRef","core_foundation::base::CFTypeRef","core_foundation::propertylist::CFPropertyListRef"]],
"core_foundation_sys":[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-TCFTypeRef-for-*const+T\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#112-120\">source</a><a href=\"#impl-TCFTypeRef-for-*const+T\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;T&gt; <a class=\"trait\" href=\"core_foundation_sys/base/trait.TCFTypeRef.html\" title=\"trait core_foundation_sys::base::TCFTypeRef\">TCFTypeRef</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const T</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.as_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#113-115\">source</a><a href=\"#method.as_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"core_foundation_sys/base/trait.TCFTypeRef.html#tymethod.as_void_ptr\" class=\"fn\">as_void_ptr</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h4></section><section id=\"method.from_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#117-119\">source</a><a href=\"#method.from_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation_sys/base/trait.TCFTypeRef.html#tymethod.from_void_ptr\" class=\"fn\">from_void_ptr</a>(ptr: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a>) -&gt; Self</h4></section></div></details>","TCFTypeRef","core_foundation_sys::array::CFArrayRef","core_foundation_sys::attributed_string::CFAttributedStringRef","core_foundation_sys::bag::CFBagRef","core_foundation_sys::base::CFAllocatorRef","core_foundation_sys::base::CFNullRef","core_foundation_sys::base::CFTypeRef","core_foundation_sys::base::ConstStr255Param","core_foundation_sys::base::ConstStringPtr","core_foundation_sys::bit_vector::CFBitVectorRef","core_foundation_sys::characterset::CFCharacterSetRef","core_foundation_sys::data::CFDataRef","core_foundation_sys::date::CFDateRef","core_foundation_sys::dictionary::CFDictionaryRef","core_foundation_sys::locale::CFLocaleRef","core_foundation_sys::number::CFBooleanRef","core_foundation_sys::number::CFNumberRef","core_foundation_sys::set::CFSetRef","core_foundation_sys::string::CFStringRef","core_foundation_sys::timezone::CFTimeZoneRef","core_foundation_sys::url::CFURLRef","core_foundation_sys::uuid::CFUUIDRef"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-TCFTypeRef-for-*mut+T\" class=\"impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#122-130\">source</a><a href=\"#impl-TCFTypeRef-for-*mut+T\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;T&gt; <a class=\"trait\" href=\"core_foundation_sys/base/trait.TCFTypeRef.html\" title=\"trait core_foundation_sys::base::TCFTypeRef\">TCFTypeRef</a> for <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*mut T</a></h3></section></summary><div class=\"impl-items\"><section id=\"method.as_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#123-125\">source</a><a href=\"#method.as_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"core_foundation_sys/base/trait.TCFTypeRef.html#tymethod.as_void_ptr\" class=\"fn\">as_void_ptr</a>(&amp;self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a></h4></section><section id=\"method.from_void_ptr\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/core_foundation_sys/base.rs.html#127-129\">source</a><a href=\"#method.from_void_ptr\" class=\"anchor\">§</a><h4 class=\"code-header\">unsafe fn <a href=\"core_foundation_sys/base/trait.TCFTypeRef.html#tymethod.from_void_ptr\" class=\"fn\">from_void_ptr</a>(ptr: <a class=\"primitive\" href=\"https://doc.rust-lang.org/1.79.0/std/primitive.pointer.html\">*const </a><a class=\"type\" href=\"https://doc.rust-lang.org/1.79.0/std/os/raw/type.c_void.html\" title=\"type std::os::raw::c_void\">c_void</a>) -&gt; Self</h4></section></div></details>","TCFTypeRef","core_foundation_sys::array::CFMutableArrayRef","core_foundation_sys::attributed_string::CFMutableAttributedStringRef","core_foundation_sys::bag::CFMutableBagRef","core_foundation_sys::base::StringPtr","core_foundation_sys::binary_heap::CFBinaryHeapRef","core_foundation_sys::bit_vector::CFMutableBitVectorRef","core_foundation_sys::bundle::CFBundleRef","core_foundation_sys::bundle::CFPlugInRef","core_foundation_sys::calendar::CFCalendarRef","core_foundation_sys::characterset::CFMutableCharacterSetRef","core_foundation_sys::data::CFMutableDataRef","core_foundation_sys::date_formatter::CFDateFormatterRef","core_foundation_sys::dictionary::CFMutableDictionaryRef","core_foundation_sys::error::CFErrorRef","core_foundation_sys::file_security::CFFileSecurityRef","core_foundation_sys::filedescriptor::CFFileDescriptorRef","core_foundation_sys::mach_port::CFMachPortRef","core_foundation_sys::messageport::CFMessagePortRef","core_foundation_sys::notification_center::CFNotificationCenterRef","core_foundation_sys::number_formatter::CFNumberFormatterRef","core_foundation_sys::plugin::CFPlugInInstanceRef","core_foundation_sys::runloop::CFRunLoopRef","core_foundation_sys::runloop::CFRunLoopSourceRef","core_foundation_sys::runloop::CFRunLoopObserverRef","core_foundation_sys::runloop::CFRunLoopTimerRef","core_foundation_sys::set::CFMutableSetRef","core_foundation_sys::socket::CFSocketRef","core_foundation_sys::stream::CFReadStreamRef","core_foundation_sys::stream::CFWriteStreamRef","core_foundation_sys::string::CFMutableStringRef","core_foundation_sys::string_tokenizer::CFStringTokenizerRef","core_foundation_sys::tree::CFTreeRef","core_foundation_sys::url_enumerator::CFURLEnumeratorRef","core_foundation_sys::user_notification::CFUserNotificationRef","core_foundation_sys::xml_node::CFXMLNodeRef","core_foundation_sys::xml_parser::CFXMLParserRef"]],
"system_configuration_sys":[]
};if (window.register_type_impls) {window.register_type_impls(type_impls);} else {window.pending_type_impls = type_impls;}})()