(function() {var implementors = {};
implementors['rotor_stream'] = ["impl&lt;P: <a class='trait' href='rotor_stream/trait.Protocol.html' title='rotor_stream::Protocol'>Protocol</a>&gt; <a class='trait' href='rotor/machine/trait.Machine.html' title='rotor::machine::Machine'>Machine</a> for <a class='struct' href='rotor_stream/struct.Stream.html' title='rotor_stream::Stream'>Stream</a>&lt;P&gt;","impl&lt;M, A, S&gt; <a class='trait' href='rotor/machine/trait.Machine.html' title='rotor::machine::Machine'>Machine</a> for <a class='enum' href='rotor_stream/enum.Accept.html' title='rotor_stream::Accept'>Accept</a>&lt;M, A&gt; <span class='where'>where A: <a class='trait' href='mio/io/trait.TryAccept.html' title='mio::io::TryAccept'>TryAccept</a>&lt;Output=S&gt; + <a class='trait' href='mio/io/trait.Evented.html' title='mio::io::Evented'>Evented</a> + <a class='trait' href='https://doc.rust-lang.org/nightly/core/any/trait.Any.html' title='core::any::Any'>Any</a>, S: <a class='trait' href='rotor_stream/trait.StreamSocket.html' title='rotor_stream::StreamSocket'>StreamSocket</a>, M: <a class='trait' href='rotor/machine/trait.Machine.html' title='rotor::machine::Machine'>Machine</a> + <a class='trait' href='rotor_stream/trait.Accepted.html' title='rotor_stream::Accepted'>Accepted</a>&lt;S&gt;</span>",];

            if (window.register_implementors) {
                window.register_implementors(implementors);
            } else {
                window.pending_implementors = implementors;
            }
        
})()
