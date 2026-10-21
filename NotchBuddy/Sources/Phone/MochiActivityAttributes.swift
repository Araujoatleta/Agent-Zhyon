import ActivityKit

// The Live Activity's type, shared by the app (tokens) and the widget
// extension (the views). Started and updated from the Mac, through the relay;
// a build without the Mac's push link (the sideload build) starts its own for
// the Windows or Linux computer, while Coucou is open (PCLiveActivity).

struct MochiActivityAttributes: ActivityAttributes {
    typealias ContentState = MochiActivityState
}
